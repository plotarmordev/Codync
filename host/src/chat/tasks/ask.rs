//! The bot asks one of its working items a question and gets the answer (`ask_work`): the question goes into the
//! task's running turn as `tell_work`'s `now` does, marked as the bot asking; the task's next message after it
//! (`answered`, from its actor as the message ends) is the call's result, not its report. The call waits [`WAIT`];
//! after that it says there is no answer yet, and the answer, when it comes, is a turn of its own in the bot's chat
//! (`Cmd::TaskAnswer`), as a `message_bot` reply is. Refused for an item that waits or ended, one the user started,
//! a routine's run and another bot's request; one question at a time a task.

use super::items::{self, Status};
use super::thread::{self, Refused};
use super::{MAX_BRIEF, prompts};
use crate::LockExt;
use crate::agent::bot::Cmd;
use crate::analytics::{self, Event};
use crate::hub::Hub;
use crate::store::BotConfig;
use anyhow::Result;
use serde::Serialize;
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::oneshot;

/// The tool's name.
pub const TOOL: &str = "ask_work";
/// How long the bot's call waits for the task's answer.
pub const WAIT: Duration = Duration::from_mins(1);

/// A question a task was asked and hasn't answered yet (`Tasks::asks`, by task).
pub(super) struct Pending {
    bot: String,
    item: String,
    title: String,
    question: String,
    /// The item's hops: an item the bot adds in the turn a late answer starts carries them on.
    hops: u8,
    /// It reached the task's agent: the task's next message is its answer.
    heard: bool,
    /// The bot's call, while it still waits for the answer.
    waiter: Option<oneshot::Sender<String>>,
}

/// How a question came out: `work_asked`'s `outcome`.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
enum Outcome {
    Answered,
    /// Not within [`WAIT`] (or the item ended first): any answer comes later, as a turn of its own.
    Late,
    Refused,
}

fn counted(hub: &Hub, outcome: Outcome) {
    let properties = json!({"outcome": outcome});
    analytics::capture(hub, Event::WorkAsked, properties.as_object().cloned().unwrap_or_default());
}

/// `ask_work {item, question}` from `bot`'s chat: `{itemId, answer}`, or `{itemId, status: "no answer yet"}`.
pub(super) async fn ask(hub: &Arc<Hub>, bot: &str, args: &Value) -> Result<Value> {
    let (h, b, a) = (hub.clone(), bot.to_owned(), args.clone());
    let put = tokio::task::spawn_blocking(move || put(&h, &b, &a)).await?;
    let (item, task, answer) = put.inspect_err(|_| counted(hub, Outcome::Refused))?;
    let answer = wait(hub, &task, answer).await;
    counted(hub, if answer.is_some() { Outcome::Answered } else { Outcome::Late });
    Ok(match answer {
        Some(answer) => json!({"itemId": item, "answer": answer}),
        None => json!({"itemId": item, "status": "no answer yet",
            "note": "Its answer comes to you later as a message of its own, if it answers before its item ends."}),
    })
}

/// Puts the question to the task of the item `args` names: the item's id, its task's, and where the answer comes.
fn put(hub: &Arc<Hub>, bot: &str, args: &Value) -> Result<(String, String, oneshot::Receiver<String>)> {
    let text = |key: &str| args[key].as_str().map(str::trim).filter(|t| !t.is_empty());
    let invalid = |why: String| Refused::error("invalid", why);
    let id = text("item").ok_or_else(|| invalid("item is required: its id, from work_status".into()))?;
    let question = text("question").ok_or_else(|| invalid("question is required".into()))?;
    if question.len() > MAX_BRIEF {
        return Err(invalid(format!("question is at most {MAX_BRIEF} bytes")));
    }
    super::tools::the_bots(hub, bot, id)?;
    let (rx, task, entry, queued) = items::change(hub, bot, |items| {
        let item = items::find(items, id)?;
        thread::writable(item)?;
        let title = &item.title;
        match item.status {
            Status::Working => {}
            Status::Waiting => {
                let why =
                    format!("\"{title}\" hasn't started yet: nothing can answer. tell_work passes words on to it");
                return Err(Refused::error("not_started", why));
            }
            _ => return Err(Refused::error("ended", format!("\"{title}\" has ended: its report came to you already"))),
        }
        let task = thread::task_of(&hub.store, item)?;
        let (tx, rx) = oneshot::channel();
        {
            let mut asks = hub.tasks.asks.locked();
            // A call still waits for the answer (one the bot's stopped turn dropped doesn't).
            if asks.get(&task.id).is_some_and(|p| p.waiter.as_ref().is_some_and(|w| !w.is_closed())) {
                return Err(Refused::error("busy", format!("\"{title}\" has a question of yours to answer already")));
            }
            let pending = Pending {
                bot: bot.to_owned(),
                item: item.id.clone(),
                title: title.clone(),
                question: question.to_owned(),
                hops: item.hops,
                heard: false,
                waiter: Some(tx),
            };
            asks.insert(task.id.clone(), pending);
        }
        // Never kept with the item (`relayed`): a session that starts it again would read it as an instruction.
        let posted = thread::post_to(hub, &task, json!({"text": question, "from": "host", "relay": true, "ask": true}));
        let (entry, queued) = posted.inspect_err(|_| drop(hub.tasks.asks.locked().remove(&task.id)))?;
        Ok((rx, task.id, entry.id, queued))
    })?;
    if queued {
        hub.send_cmd(&task, Cmd::Steer { entry_id: entry })?;
    }
    Ok((id.to_owned(), task, rx))
}

/// The task's answer, if it comes within [`WAIT`]; past that the question stays, and its answer is late.
async fn wait(hub: &Hub, task: &str, mut answer: oneshot::Receiver<String>) -> Option<String> {
    if let Ok(answered) = tokio::time::timeout(WAIT, &mut answer).await {
        return answered.ok();
    }
    // From here an answer goes to the bot's chat; one sent a moment ago is still this call's.
    if let Some(pending) = hub.tasks.asks.locked().get_mut(task) {
        pending.waiter = None;
    }
    answer.try_recv().ok()
}

/// The entries `entries` reached task `cfg`'s agent (its turn began with them, or one joined it): a question of its
/// bot's among them makes its next message the answer.
pub fn heard(hub: &Hub, cfg: &BotConfig, entries: &[String]) {
    mark(hub, cfg, entries, true);
}

/// The question in `entry` didn't join the running turn after all: it waits for a turn of its own.
pub fn unheard(hub: &Hub, cfg: &BotConfig, entry: &str) {
    mark(hub, cfg, &[entry.to_owned()], false);
}

fn mark(hub: &Hub, cfg: &BotConfig, entries: &[String], heard: bool) {
    if !cfg.is_task() || !hub.tasks.asks.locked().contains_key(&cfg.id) {
        return;
    }
    if entries.iter().any(|id| hub.store.entry(id).is_some_and(|e| e.data["ask"] == true))
        && let Some(pending) = hub.tasks.asks.locked().get_mut(&cfg.id)
    {
        pending.heard = heard;
    }
}

/// Task `cfg` wrote `text`, a message of its turn: when it answers its bot's question, the answer goes to the bot
/// (its call, or a turn of its own once the call gave up). Whether it did: then it isn't the turn's report.
pub fn answered(hub: &Hub, cfg: &BotConfig, text: &str) -> bool {
    let pending = {
        let mut asks = hub.tasks.asks.locked();
        if !asks.get(&cfg.id).is_some_and(|p| p.heard) {
            return false;
        }
        let Some(mut pending) = asks.remove(&cfg.id) else { return false };
        // Sent under the lock, so the call that gives up either gets it or leaves it to its bot's chat (`wait`).
        match pending.waiter.take().map(|waiter| waiter.send(text.trim().to_owned())) {
            Some(Ok(())) => return true,
            Some(Err(_)) | None => pending,
        }
    };
    let prompt = prompts::late_answer(&pending.title, &pending.item, &pending.question, text.trim());
    if let Err(error) = hub.send_cmd(&pending.bot, Cmd::TaskAnswer { prompt, hops: pending.hops }) {
        tracing::warn!(bot = pending.bot, error = format!("{error:#}"), "couldn't give a bot its task's late answer");
    }
    true
}

/// `task`'s item ended or was set aside: a question it hadn't answered goes, and a call still waiting hears so.
pub(super) fn forget(hub: &Hub, task: &str) {
    hub.tasks.asks.locked().remove(task);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::calls::context::tests::{bot, hub};
    use crate::store::{EntryKind, Lane};

    fn asked(hub: &Hub, task: &str) -> (oneshot::Receiver<String>, String) {
        let (tx, rx) = oneshot::channel();
        let pending = Pending {
            bot: "b1".into(),
            item: "i1".into(),
            title: "Docs".into(),
            question: "how far?".into(),
            hops: 0,
            heard: false,
            waiter: Some(tx),
        };
        hub.tasks.asks.locked().insert(task.into(), pending);
        let data = json!({"text": "how far?", "from": "host", "relay": true, "ask": true});
        let entry = hub.add_entry(&Lane::main(task), EntryKind::User, 1, &data).expect("posted");
        (rx, entry.id)
    }

    #[tokio::test]
    async fn the_tasks_next_message_after_the_question_is_the_answer() {
        let hub = hub();
        let task = BotConfig { task_of: Some("b1".into()), ..bot("task-1", "Mika", "/tmp") };
        hub.store.save_bot(&task).expect("saved");
        let (mut rx, entry) = asked(&hub, "task-1");
        assert!(!answered(&hub, &task, "still reading"), "before the question reached it");
        heard(&hub, &task, std::slice::from_ref(&entry));
        unheard(&hub, &task, &entry);
        assert!(!answered(&hub, &task, "a steer it didn't take"));
        heard(&hub, &task, &[entry]);
        assert!(answered(&hub, &task, " Half done. "));
        assert_eq!(rx.try_recv().expect("the answer"), "Half done.");
        assert!(!answered(&hub, &task, "the report"), "one message answers it");
    }

    #[test]
    fn a_question_is_only_for_the_bots_own_working_item() {
        let hub = hub();
        hub.store.save_bot(&bot("b1", "Mika", "/tmp")).expect("saved");
        let items = json!([
            {"id": "w", "title": "Docs", "brief": "x", "status": "waiting", "order": 1, "createdAt": 0},
            {"id": "d", "title": "Login", "brief": "x", "status": "done", "order": 0, "createdAt": 0, "endedAt": 1},
            {"id": "u", "title": "Mine", "brief": "x", "status": "working", "order": 2, "createdAt": 0,
                "task": "task-u", "source": {"kind": "user"}},
            {"id": "r", "title": "Morning", "brief": "x", "status": "working", "order": 3, "createdAt": 0,
                "task": "task-r", "source": {"kind": "routine", "id": "r", "run": "u"}},
        ]);
        hub.store.kv_set(&format!("{}b1", items::PREFIX), &items.to_string()).expect("saved");
        let category = |item: &str, question: &str| {
            let error = put(&hub, "b1", &json!({"item": item, "question": question})).expect_err("refused");
            error.downcast_ref::<Refused>().map(|refused| refused.category)
        };
        assert_eq!(category("w", "how far?"), Some("not_started"));
        assert_eq!(category("d", "how far?"), Some("ended"));
        assert_eq!(category("u", "how far?"), Some("users"));
        assert_eq!(category("r", "how far?"), Some("read_only"));
        assert_eq!(category("w", " "), Some("invalid"));
        assert!(hub.tasks.asks.locked().is_empty(), "nothing waits for an answer");
    }

    #[tokio::test]
    async fn a_call_that_gave_up_leaves_the_answer_to_the_bots_chat() {
        let hub = hub();
        let task = BotConfig { task_of: Some("b1".into()), ..bot("task-1", "Mika", "/tmp") };
        let (rx, entry) = asked(&hub, "task-1");
        // As `wait` does past its time.
        hub.tasks.asks.locked().get_mut("task-1").expect("pending").waiter = None;
        drop(rx);
        heard(&hub, &task, &[entry]);
        assert!(answered(&hub, &task, "Done soon."), "late, to the bot's chat");
        assert!(!hub.tasks.asks.locked().contains_key("task-1"));
        let (rx, _) = asked(&hub, "task-1");
        forget(&hub, "task-1");
        let mut rx = rx;
        assert!(rx.try_recv().is_err(), "its item ended: the waiting call hears so");
    }
}
