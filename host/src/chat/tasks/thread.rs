//! Talking to a thread: the user writes to the task that works, or worked, one of the bot's own items, as to a
//! chat. The task is a bot with an actor, so the message is a message in the task's own chat: it waits behind a
//! running turn, can be sent now or removed, and starts a turn when none runs. An item that ended is reopened by
//! the first message, and reports again when that turn ends. The bot's chat passes words on to a task here too
//! (`tell`). A routine's run and another bot's request take none: their result has an owner already.

use super::items::{self, Item, Source, Status};
use super::{prompts, worktree};
use crate::LockExt;
use crate::agent::bot::Cmd;
use crate::hub::{BotStatus, Hub};
use crate::store::{BotConfig, Entry, Store, now_ms};
use anyhow::{Result, anyhow};
use serde_json::{Value, json};
use std::path::Path;
use std::sync::Arc;

/// What an item keeps of the user's words for its next report at most.
const TOLD_KEPT: usize = 20;

/// A message a thread didn't take, with why for analytics (`category`).
#[derive(Debug)]
pub struct Refused {
    pub category: &'static str,
    message: String,
}

impl Refused {
    pub fn error(category: &'static str, message: String) -> anyhow::Error {
        Self { category, message }.into()
    }
}

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Refused {}

/// Whether `item` takes words at all: the bot's own work does; a routine's run and another bot's request have an
/// owner for their result already.
fn writable(item: &Item) -> Result<()> {
    let whose = match &item.source {
        Source::Bot | Source::User => return Ok(()),
        Source::Routine { .. } => "a routine's run",
        Source::Request { .. } => "another bot's request",
    };
    let why = format!("\"{}\" is {whose}: it can be read and stopped, not written to", item.title);
    Err(Refused::error("read_only", why))
}

/// The task that takes the user's messages for `item`, or why it takes none.
fn task_of(store: &Store, item: &Item) -> Result<BotConfig> {
    writable(item)?;
    let title = &item.title;
    let Some(task) = item.task.as_deref() else {
        let why = format!("\"{title}\" hasn't started yet: there is nothing to write to");
        return Err(Refused::error("not_started", why));
    };
    let row = store.bot(task)?.filter(|row| !row.deleted);
    let gone = || Refused::error("gone", format!("\"{title}\" ended too long ago to write to: add new work instead"));
    Ok(row.ok_or_else(gone)?.config)
}

/// The task behind `bot`'s item `id`, for what is done to a message that waits in its thread (`steer`, `unqueue`).
/// The caller names the bot and the item, never a task: the item must be this bot's, and its own work.
pub fn task(hub: &Hub, bot: &str, id: &str) -> Result<String> {
    let mut items = items::load(&hub.store, bot);
    Ok(task_of(&hub.store, items::find(&mut items, id)?)?.id)
}

/// The user's messages that wait for their turn in `task`'s thread.
fn waiting(store: &Store, task: &str) -> Vec<Entry> {
    store.queued_messages(task).unwrap_or_else(|error| {
        tracing::warn!(task, error = format!("{error:#}"), "couldn't read a thread's waiting messages");
        Vec::new()
    })
}

/// Whether a message of the user's still waits for its turn in `task`'s thread: its item isn't over until that
/// turn is, and nothing ends the task meanwhile.
pub(super) fn unheard(store: &Store, task: &str) -> bool {
    !waiting(store, task).is_empty()
}

/// `item`'s task is closed for the item to start again (it gave way, or got a new brief): what the user wrote to
/// its thread and no turn took yet goes with the item, as what they said since.
pub(super) fn carry(store: &Store, item: &mut Item, task: &str) {
    // What its chat passed on is with the item already (`tell`).
    // ponytail: a waiting message's files stay behind (only their names go); move them to the new task if users
    // miss them.
    for entry in waiting(store, task).into_iter().filter(|e| e.data["relay"] != true) {
        note(&mut item.notes, &carried(&entry.data));
    }
}

/// A waiting message as the item that starts again keeps it: its words and the names of the files left behind.
fn carried(data: &Value) -> String {
    let left = data["attachments"].as_array().is_some_and(|a| !a.is_empty());
    format!("{}{}", written(data), if left { " (those files stayed behind)" } else { "" })
}

/// One message the user wrote to a task, as its item keeps it: the words, and the names of the files sent with them.
pub fn written(data: &Value) -> String {
    let text = data["text"].as_str().unwrap_or_default().trim();
    let names: Vec<&str> =
        data["attachments"].as_array().into_iter().flatten().filter_map(|a| a["name"].as_str()).collect();
    match (text.is_empty(), names.is_empty()) {
        (_, true) => text.to_owned(),
        (true, false) => format!("[sent files: {}]", names.join(", ")),
        (false, false) => format!("{text} [sent files: {}]", names.join(", ")),
    }
}

fn note(notes: &mut Vec<String>, words: &str) {
    let words = words.trim();
    if !words.is_empty() && notes.last().map(String::as_str) != Some(words) {
        notes.push(words.to_owned());
    }
}

/// An item that ended works again, in the task it had: under Threads, timed from now, counted against the bot's
/// agents at once even past the ceiling (the user asked). Its worktree, if it was kept, is its folder again. A
/// restart that cuts this turn off ends it: the work was done once, and isn't done again from its brief.
fn reopen(hub: &Hub, owner: &BotConfig, item: &mut Item, task: &mut BotConfig) -> Result<()> {
    item.said = item.result.take().or(item.said.take());
    item.status = Status::Working;
    item.restarted = true;
    item.started_at = Some(now_ms());
    item.ended_at = None;
    item.failure = None;
    // A worktree being looked over (`Settling`: its item just ended) is in use again too, which the look then leaves.
    // ponytail: that look may still remove an untouched one; the task then fails to start in it and says so. Hold
    // the look until no message can come if that ever shows.
    if let Some(kept) = item.worktree.as_mut().filter(|w| worktree::usable(w)) {
        kept.left = worktree::Left::InUse;
    } else if !Path::new(&task.cwd).is_dir() {
        // Its worktree had nothing in it and went: the folder a task without one works in.
        task.cwd.clone_from(item.setup.cwd.as_ref().unwrap_or(&owner.cwd));
        let _g = hub.emit_lock.locked();
        hub.store.save_bot(task)?;
    }
    Ok(())
}

/// The user writes `text` to the thread of `bot`'s item `id`, with the files uploaded to that thread (`uploads`,
/// their ids). A turn that runs there makes it wait, as in a chat; an item that ended is reopened and its task's
/// actor started again, which resumes its session where the agent can. `nonce`: the client's, so a message sent
/// twice is one. Answers `{entry, reopened, queued}`: the message as the thread's transcript holds it, whether it
/// reopened the item, and whether it waits behind a running turn (both null for a message sent before).
pub fn message(hub: &Arc<Hub>, bot: &str, id: &str, text: &str, uploads: &[String], nonce: &str) -> Result<Value> {
    let owner = hub.store.bot(bot)?.filter(|row| !row.deleted).ok_or_else(|| anyhow!("unknown bot"))?.config;
    // Under the box's lock to the end: the item's end and the dispatcher's look at its task come before all of
    // this or after it, so neither ends a task a message is on its way to.
    let (entry, reopened, queued) = items::change(hub, bot, |items| {
        let item = items::find(items, id)?;
        let ended = item.status.ended();
        let mut task = task_of(&hub.store, item)?;
        if let Some(sent) = Some(nonce).filter(|n| !n.is_empty()).and_then(|n| hub.store.find_by_nonce(&task.id, n)) {
            return Ok((sent, None, None));
        }
        // Its task may be on an item that followed it by now: the message is for that session all the same.
        let reopened = ended && items::of_task(items, &task.id).is_none();
        if reopened {
            reopen(hub, &owner, items::find(items, id)?, &mut task)?;
        }
        let root = crate::chat::uploads::root(&task);
        let attachments = crate::chat::uploads::resolve_all(&root, uploads)?;
        let data = json!({"text": text, "clientNonce": nonce, "attachments": attachments});
        let (entry, queued) = post_to(hub, &task, data)?;
        Ok((entry, Some(reopened), Some(queued)))
    })?;
    if reopened == Some(true) {
        // It shows under Threads again, and what follows the bot from outside its chat counts it working.
        super::dispatch(hub, bot);
    }
    Ok(json!({"entry": entry, "reopened": reopened, "queued": queued}))
}

/// Posts `data` to `task`'s chat as a message, its actor started again if it ended. Answers the entry, and whether
/// it waits behind a running turn.
fn post_to(hub: &Arc<Hub>, task: &BotConfig, data: Value) -> Result<(Entry, bool)> {
    if !hub.has_actor(&task.id) {
        hub.spawn_bot(task.clone());
    }
    let queued = matches!(hub.runtime(&task.id).status, BotStatus::Working | BotStatus::NeedsInput);
    Ok((crate::chat::messages::post(hub, task, data)?, queued))
}

/// Where words passed on to an item went (`tell`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Told {
    /// With the waiting item, for its task to read when it starts.
    Waiting,
    /// A message in the working item's task; `queued`: it waits behind a running turn.
    Working { queued: bool },
}

/// The bot passes `text` on to its item `id` (`tell_work`), which keeps it (`Item::relayed`) so a session that
/// starts the item, or starts it again, reads it. A working item also gets it as a message in its task from the chat
/// (`from: host`, `relay`; every client quotes it there, as what the task was told), which waits behind a running
/// turn as the user's does and keeps the item going until it is taken. `now` hands it to that turn at its next step,
/// or stops the turn for it where the agent takes no message mid-turn (`agent::bot::steer`).
pub fn tell(hub: &Arc<Hub>, bot: &str, id: &str, text: &str, now: bool) -> Result<Told> {
    let (told, steer) = items::change(hub, bot, |items| {
        let item = items::find(items, id)?;
        writable(item)?;
        match item.status {
            Status::Waiting => {
                note(&mut item.relayed, text);
                Ok((Told::Waiting, None))
            }
            Status::Working => {
                let task = task_of(&hub.store, item)?;
                note(&mut item.relayed, text);
                let (entry, queued) = post_to(hub, &task, json!({"text": text, "from": "host", "relay": true}))?;
                Ok((Told::Working { queued }, (now && queued).then_some((task.id, entry.id))))
            }
            _ => {
                let why = format!(
                    "\"{}\" has ended: its report came to you already. For more of that work, add a new item",
                    item.title
                );
                Err(Refused::error("ended", why))
            }
        }
    })?;
    if let Some((task, entry_id)) = steer {
        hub.send_cmd(&task, Cmd::Steer { entry_id })?;
    }
    Ok(told)
}

/// A turn of task `cfg` begins for what the user wrote to its thread (`words`, each message's): the item's next
/// report ends with them, and a session that starts the item again reads them.
pub fn heard(hub: &Hub, cfg: &BotConfig, words: &[String]) {
    super::update(hub, cfg, "what the user wrote to it", |item| {
        for words in words.iter().map(|w| w.trim()).filter(|w| !w.is_empty()) {
            note(&mut item.notes, words);
            item.told.push(words.to_owned());
        }
        let over = item.told.len().saturating_sub(TOLD_KEPT);
        item.told.drain(..over);
    });
}

/// What [`heard`] kept of `words` goes: those messages went back to wait (the agent dropped them with its turn), and
/// their own turn hears them again.
pub fn unhear(hub: &Hub, cfg: &BotConfig, words: &[String]) {
    super::update(hub, cfg, "what the user wrote to it", |item| {
        for words in words.iter().rev().map(|words| words.trim()) {
            for kept in [&mut item.told, &mut item.notes] {
                if kept.last().is_some_and(|last| last == words) {
                    kept.pop();
                }
            }
        }
    });
}

/// What a new session of task `cfg` is told before the user's message, when the session that did its item can't
/// be picked up: the item, what the user said about it and what it reported.
pub fn picked_up(store: &Store, cfg: &BotConfig) -> Option<String> {
    super::item_of(store, cfg).map(|item| prompts::picked_up(&item))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::calls::context::tests::{bot, hub};

    fn item(more: &Value) -> Item {
        let mut item = json!({"id": "i1", "title": "Fix login", "brief": "x", "status": "done", "order": 1,
            "createdAt": 0, "task": "task-1"});
        item.as_object_mut().expect("an object").extend(more.as_object().cloned().unwrap_or_default());
        serde_json::from_value(item).expect("an item")
    }

    #[test]
    fn only_the_bots_own_work_whose_task_is_there_takes_a_message() {
        let hub = hub();
        let task = BotConfig { task_of: Some("b1".into()), ..bot("task-1", "Mika", "/tmp") };
        let refusal = |item: &Item| task_of(&hub.store, item).expect_err("refused");
        let refused = |item: &Item| refusal(item).to_string();
        let category = |item: &Item| refusal(item).downcast_ref::<Refused>().map(|refused| refused.category);
        assert_eq!(category(&item(&json!({}))), Some("gone"));
        assert_eq!(category(&item(&json!({"source": {"kind": "request"}}))), Some("read_only"));
        assert_eq!(category(&item(&json!({"status": "waiting", "task": null}))), Some("not_started"));
        assert_eq!(refused(&item(&json!({}))), "\"Fix login\" ended too long ago to write to: add new work instead");
        hub.store.save_bot(&task).expect("saved");
        assert_eq!(task_of(&hub.store, &item(&json!({}))).expect("its task").id, "task-1");
        assert_eq!(
            refused(&item(&json!({"source": {"kind": "routine", "id": "r", "run": "u"}}))),
            "\"Fix login\" is a routine's run: it can be read and stopped, not written to"
        );
        assert_eq!(
            refused(&item(&json!({"source": {"kind": "request"}}))),
            "\"Fix login\" is another bot's request: it can be read and stopped, not written to"
        );
        assert_eq!(
            refused(&item(&json!({"status": "waiting", "task": null}))),
            "\"Fix login\" hasn't started yet: there is nothing to write to"
        );
    }

    #[test]
    fn a_message_of_files_alone_is_told_by_their_names() {
        let files = json!([{"name": "shot.png"}, {"name": "log.txt"}]);
        assert_eq!(written(&json!({"text": "", "attachments": files})), "[sent files: shot.png, log.txt]");
        assert_eq!(written(&json!({"text": " see ", "attachments": files})), "see [sent files: shot.png, log.txt]");
        assert_eq!(written(&json!({"text": "see", "attachments": []})), "see");
        let carried_files = carried(&json!({"text": "see", "attachments": files}));
        assert_eq!(carried_files, "see [sent files: shot.png, log.txt] (those files stayed behind)");
        assert_eq!(carried(&json!({"text": "see"})), "see");
    }

    #[tokio::test]
    async fn files_sent_to_a_task_are_its_own_uploads() {
        let hub = hub();
        hub.store.save_bot(&bot("b1", "Mika", "/tmp")).expect("saved");
        let task = BotConfig { task_of: Some("b1".into()), ..bot("task-1", "Mika", "/tmp") };
        hub.store.save_bot(&task).expect("saved");
        crate::chat::tasks::put_working_for_test(&hub.store, "b1", "task-1");
        let root = crate::chat::uploads::root(&task);
        let upload = uuid::Uuid::new_v4().to_string();
        crate::chat::uploads::append(&root, &upload, "shot.png", 0, b"png", true).expect("uploaded");
        let sent = message(&hub, "b1", "i1", "", std::slice::from_ref(&upload), "n1").expect("sent");
        assert_eq!(sent["entry"]["botId"], "task-1");
        assert_eq!(sent["entry"]["data"]["attachments"][0]["name"], "shot.png");
        let unknown = [uuid::Uuid::new_v4().to_string()];
        assert!(message(&hub, "b1", "i1", "", &unknown, "n2").is_err(), "only the task's own uploads");
    }

    #[test]
    fn a_reopened_item_works_again_and_keeps_what_it_reported() {
        let hub = hub();
        let owner = bot("b1", "Mika", "/tmp");
        let mut task = BotConfig { task_of: Some("b1".into()), ..bot("task-1", "Mika", "/nowhere/it/went") };
        let mut item = item(&json!({"result": "All green.", "endedAt": 5, "startedAt": 1}));
        reopen(&hub, &owner, &mut item, &mut task).expect("reopened");
        assert_eq!((item.status, item.ended_at, item.result.as_deref()), (Status::Working, None, None));
        assert!(item.restarted, "cut off now, it ends: it isn't done again from its brief");
        assert_eq!(item.said.as_deref(), Some("All green."));
        assert!(item.started_at.is_some_and(|at| at > 1), "timed from now");
        assert_eq!(task.cwd, "/tmp", "a folder that went is the bot's again");
    }

    #[test]
    fn what_the_user_wrote_stays_with_the_item_for_its_report_and_its_next_session() {
        let hub = hub();
        let cfg = BotConfig { task_of: Some("b1".into()), ..bot("task-1", "Mika", "/tmp") };
        crate::chat::tasks::put_working_for_test(&hub.store, "b1", "task-1");
        let words: Vec<String> = (0..=TOLD_KEPT).map(|n| format!(" word {n} ")).collect();
        heard(&hub, &cfg, &words);
        let item = items::load(&hub.store, "b1").remove(0);
        assert_eq!(item.told.len(), TOLD_KEPT, "the newest are kept");
        assert_eq!((item.told[0].as_str(), item.notes[0].as_str()), ("word 1", "word 0"));
    }

    #[test]
    fn words_that_went_back_to_wait_are_the_items_once() {
        let hub = hub();
        let cfg = BotConfig { task_of: Some("b1".into()), ..bot("task-1", "Mika", "/tmp") };
        crate::chat::tasks::put_working_for_test(&hub.store, "b1", "task-1");
        let words = |said: &[&str]| said.iter().map(|w| (*w).to_owned()).collect::<Vec<_>>();
        heard(&hub, &cfg, &words(&["earlier"]));
        // Two messages joined a turn its agent dropped: their own turn hears them again.
        heard(&hub, &cfg, &words(&["one", "two"]));
        unhear(&hub, &cfg, &words(&["one", "two"]));
        heard(&hub, &cfg, &words(&["one", "two"]));
        let item = items::load(&hub.store, "b1").remove(0);
        assert_eq!(item.told, words(&["earlier", "one", "two"]));
        assert_eq!(item.notes, words(&["earlier", "one", "two"]));
    }
}
