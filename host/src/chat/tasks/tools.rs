//! The `chat` server's work tools (`add_work`, `steer_work`, `tell_work`, `ask_work`, `work_status`, `recall_work`), and
//! task itself may not call.

use super::items::{self, FolderKind, Item, Shown, Status};
use super::steer::{self, Steer};
use super::thread::{self, Told};
use super::worktree::PlaceChoice;
use super::{MAX_BRIEF, MAX_HOPS, MAX_TITLE, MAX_WAITING, dispatch, recall, sources};
use crate::LockExt;
use crate::analytics::{self, Event};
use crate::hub::Hub;
use crate::store::now_ms;
use anyhow::{Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;

/// The `chat` server's tools that are the box's.
pub const TOOLS: [&str; 6] = ["add_work", "steer_work", "tell_work", super::ask::TOOL, "work_status", "recall_work"];

/// A task can't do what only makes sense from the chat. `what`: the tool, as the refusal names it.
pub fn refuse_in_task(hub: &Hub, bot: &str, what: &str) -> Result<()> {
    if hub.store.bot(bot)?.is_some_and(|row| row.config.is_task()) {
        let properties = json!({"tool": what});
        analytics::capture(hub, Event::TaskToolRefused, properties.as_object().cloned().unwrap_or_default());
        bail!("{what} isn't available in a task: say in your report what you need, and you will handle it in the chat");
    }
    Ok(())
}

/// The bot a call to the servers that are the bot's own (`routines`, `team`) acts as. A task on a routine run or on
/// another bot's request is its bot there, as that turn was when it ran in the chat (a run saves its progress and
/// sets follow-ups; an asked bot runs its own routine); a task on the bot's own work is refused.
pub fn acting_as(hub: &Hub, bot: &str, what: &str) -> Result<String> {
    let Some(cfg) = hub.store.bot(bot)?.map(|row| row.config).filter(crate::store::BotConfig::is_task) else {
        return Ok(bot.to_owned());
    };
    if sources::source_of(&hub.store, &cfg).own() {
        refuse_in_task(hub, bot, what)?;
    }
    Ok(cfg.owner().to_owned())
}

/// The `chat` server's tools a task neither lists (`mcp::chat_tools`) nor may call: `send_message` (its last message
/// is its report), `react` (it answers no chat message), and what changes the bot, its skills or its box. What asks
/// the user or shares with them goes to the task's actor, which posts it in the bot's chat.
pub fn chat_only(name: &str) -> bool {
    use crate::chat::{fallbacks, profile, reactions, skills};
    ["send_message", reactions::TOOL, skills::TOOL].contains(&name)
        || TOOLS.contains(&name)
        || profile::TOOLS.contains(&name)
        || fallbacks::TOOLS.contains(&name)
}

/// Refuses a task a call to a [`chat_only`] tool.
pub fn refuse_chat_only(hub: &Hub, bot: &str, name: &str) -> Result<()> {
    if chat_only(name) { refuse_in_task(hub, bot, name) } else { Ok(()) }
}

pub fn tools() -> Vec<Value> {
    vec![
        json!({
            "name": "add_work",
            "description": "Put long work in your box: one of your tasks does it away from this chat, in a session of its own that sees nothing of this chat, and you stay free to answer the user. It starts when this turn ends and a place is free; until then it waits its turn. Its report comes back to you later as a message. One item per piece of work.",
            "inputSchema": {"type": "object", "properties": {
                "title": {"type": "string", "description": "A few words the user sees while it works, e.g. \"Fix the login redirect\"."},
                "brief": {"type": "string", "description": "Everything needed to do the work, standing alone (the task sees nothing of this chat): the goal, what you already know, the user's words that matter, what done looks like."},
                "after": {"type": "string", "description": "The id of an item that is working or waiting: this one is done after it, in the same session, knowing what that one did. For more of the same work."},
                "folder": {"type": "string", "description": "The full path of the folder the task works in: one of your projects, or another folder the user named. Leave out for your default project (your own folder when you have none). With after, it works where that item does."},
                "place": {"type": "string", "enum": ["here", "worktree"], "description": "Where in a git repository it works. here: in the folder itself, e.g. to build on what the item before it left there. worktree: in a git worktree of its own from the last commit, away from the user's checkout, e.g. when it edits code the user may be touching. Leave out for your worktree mode's choice. Your own folder never gets a worktree."},
            }, "required": ["title", "brief"]},
            "annotations": {"readOnlyHint": false, "idempotentHint": false},
        }),
        json!({
            "name": "steer_work",
            "description": "Change an item in your box. first: a waiting item moves to the front. now: a waiting item starts now; with every place taken a working item gives way (insteadOf, else the one started last), its session closes and it starts again later. replace: the item gets a new brief; one that is working starts again from it. stop: the item stops; what it already changed stays.",
            "inputSchema": {"type": "object", "properties": {
                "item": {"type": "string", "description": "The item's id."},
                "how": {"type": "string", "enum": ["first", "now", "replace", "stop"]},
                "brief": {"type": "string", "description": "The new brief, for replace."},
                "insteadOf": {"type": "string", "description": "For now: the id of the working item that gives way."},
            }, "required": ["item", "how"]},
            "annotations": {"readOnlyHint": false, "idempotentHint": false},
        }),
        json!({
            "name": "tell_work",
            "description": "Pass words on to one of your items without starting it again: what the user added about it (\"also check the tests\"), a fact it needs, a nudge when it goes the wrong way. A waiting item reads them when it starts. A working one gets them in its task once its current turn ends, before it reports; with now, at its next step instead (an agent that can't take a message mid-turn stops its current turn for them, so use now only when it should change course). To start work over, use steer_work replace.",
            "inputSchema": {"type": "object", "properties": {
                "item": {"type": "string", "description": "The item's id."},
                "text": {"type": "string", "description": "What to pass on, as the item should read it."},
                "now": {"type": "boolean", "description": "Hand it to a working item at its next step."},
            }, "required": ["item", "text"]},
            "annotations": {"readOnlyHint": false, "idempotentHint": false},
        }),
        json!({
            "name": super::ask::TOOL,
            "description": "Ask one of your working items a short question and get its answer: how far it is, what it found so far, what it assumed. The question goes into its running turn at its next step, marked as yours (an agent that can't take a message mid-turn stops its current turn for it); it answers in one short message and goes on with its work. Waits up to a minute: without an answer by then you get status \"no answer yet\", and the answer comes to you later as a message of its own. Not for an item that waits or ended (tell_work it, or read its report), a routine's run or another bot's request.",
            "inputSchema": {"type": "object", "properties": {
                "item": {"type": "string", "description": "The working item's id."},
                "question": {"type": "string", "description": "What to ask, as the item should read it."},
            }, "required": ["item", "question"]},
            "annotations": {"readOnlyHint": false, "idempotentHint": false},
        }),
        json!({
            "name": "work_status",
            "description": "Your box: every item that is working or waiting, and those ended in the last day, with its status, what it does now, how long, and its result.",
            "inputSchema": {"type": "object", "properties": {}},
            "annotations": {"readOnlyHint": true},
        }),
        json!({
            "name": "recall_work",
            "description": "Look back at your work, ended items too, up to 30 days. With item: its brief, what the user said and your chat passed on about it, its whole result, the items it follows and that follow it, and a page of its task's steps (a task is kept a week; for the page before, pass before: the earlierSteps it gave). With query: the items whose title, brief or result has those words. With neither: the latest ones.",
            "inputSchema": {"type": "object", "properties": {
                "item": {"type": "string", "description": "An item's id."},
                "query": {"type": "string", "description": "Words to find in an item's title, brief or result."},
                "before": {"type": "integer", "description": "With item: the earlierSteps of the page you have, for the one before it."},
            }},
            "annotations": {"readOnlyHint": true},
        }),
    ]
}

/// A work tool's call from `bot`'s chat: `ask_work` waits for its task's answer, the others run on a blocking thread.
pub async fn call(hub: &Arc<Hub>, bot: &str, name: &str, args: &Value) -> Result<Value> {
    if name == super::ask::TOOL {
        return super::ask::ask(hub, bot, args).await;
    }
    let (hub, bot, name, args) = (hub.clone(), bot.to_owned(), name.to_owned(), args.clone());
    tokio::task::spawn_blocking(move || call_now(&hub, &bot, &name, &args)).await?
}

fn call_now(hub: &Arc<Hub>, bot: &str, name: &str, args: &Value) -> Result<Value> {
    match name {
        "add_work" => add(hub, bot, args),
        "steer_work" => steer_item(hub, bot, args),
        "tell_work" => tell(hub, bot, args),
        "work_status" => status(hub, bot),
        "recall_work" => recall::call(hub, bot, args),
        _ => bail!("unknown work tool: {name}"),
    }
}

/// How `add_work` ended: `work_added`'s `outcome`.
#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "camelCase")]
enum Admission {
    Added,
    /// Not in the bot's own chat.
    Elsewhere,
    Invalid,
    /// Too many items wait already.
    Full,
    /// Too many in a row without the user.
    Hops,
    Failed,
}

fn add(hub: &Arc<Hub>, bot: &str, args: &Value) -> Result<Value> {
    let added = admit(hub, bot, args);
    let outcome = added.as_ref().map_or_else(|(reason, _)| *reason, |_| Admission::Added);
    let folder = added.as_ref().ok().map(|i| i.folder);
    let place = added.as_ref().ok().and_then(|i| i.place);
    let properties = json!({"outcome": outcome, "after": args["after"].is_string(), "source": "bot", "folder": folder,
        "place": place});
    analytics::capture(hub, Event::WorkAdded, properties.as_object().cloned().unwrap_or_default());
    let Admitted { id, place_note, .. } = added.map_err(|(_, error)| error)?;
    dispatch(hub, bot);
    let mut answer = json!({
        "itemId": id, "status": Status::Waiting,
        "note": "It starts when this turn ends and a place is free. Its report comes back to you later as a message.",
    });
    if let Some(note) = place_note {
        answer["placeNote"] = note.into();
    }
    Ok(answer)
}

/// An item `add_work` put in the box.
#[derive(Debug)]
struct Admitted {
    id: String,
    /// What its folder is to the bot.
    folder: FolderKind,
    /// Where it asked to work, as it will.
    place: Option<PlaceChoice>,
    /// Why the place it asked for doesn't apply, for the bot.
    place_note: Option<&'static str>,
}

/// The place `add_work` asks for in `folder` (`kind` to the bot), as it will apply: with why one doesn't. A worktree
/// is never made of the bot's own folder, nor outside a git repository; an item that follows another works where it
/// does.
fn placed(
    asked: Option<PlaceChoice>,
    kind: FolderKind,
    folder: &str,
    after: bool,
) -> (Option<PlaceChoice>, Option<&'static str>) {
    match asked {
        Some(PlaceChoice::Worktree) if kind == FolderKind::Home => {
            (None, Some("place worktree was left out: your own folder never gets a worktree, so it works there"))
        }
        Some(PlaceChoice::Worktree) if !super::worktree::in_repo(folder) => {
            (None, Some("place worktree was left out: the folder isn't in a git repository, so it works there"))
        }
        Some(_) if after => (asked, Some("with after, it works where that item does once that item is done")),
        _ => (asked, None),
    }
}

type Refusal = (Admission, anyhow::Error);

/// A title and a brief `add_work` takes: both there, neither too long.
fn checked<'a>(title: Option<&'a str>, brief: Option<&'a str>) -> Result<(&'a str, &'a str), Refusal> {
    let (Some(title), Some(brief)) = (title, brief) else {
        return Err((Admission::Invalid, anyhow!("title and brief are required")));
    };
    if title.chars().count() > MAX_TITLE || brief.len() > MAX_BRIEF {
        let why = anyhow!("title is at most {MAX_TITLE} characters and brief at most {MAX_BRIEF} bytes");
        return Err((Admission::Invalid, why));
    }
    Ok((title, brief))
}

/// Puts `item` last in `items`, unless too many wait already or it follows nothing that is working or waiting.
fn place_item(items: &mut Vec<Item>, mut item: Item) -> Result<String, Refusal> {
    if items.iter().filter(|i| i.status == Status::Waiting).count() >= MAX_WAITING {
        let why = anyhow!("{MAX_WAITING} items already wait: let some finish, or drop one, before adding more");
        return Err((Admission::Full, why));
    }
    if let Some(after) = &item.after
        && !items.iter().any(|i| i.id == *after && !i.status.ended())
    {
        return Err((Admission::Invalid, anyhow!("after names no item that is working or waiting: see work_status")));
    }
    item.order = items::last(items);
    let id = item.id.clone();
    items.push(item);
    Ok(id)
}

/// Puts an item in `bot`'s box, or says why not: the refusal's `work_added` outcome and its words for the bot.
fn admit(hub: &Hub, bot: &str, args: &Value) -> Result<Admitted, Refusal> {
    let failed = |error: anyhow::Error| (Admission::Failed, error);
    let row = hub.store.bot(bot).map_err(failed)?.filter(|r| !r.deleted);
    let cfg = row.ok_or_else(|| failed(anyhow!("unknown bot")))?.config;
    // A room's turn, another bot's request, a routine run: their reply goes elsewhere, and so would the report's.
    let own_chat = hub.runtime(bot).lane.is_some_and(|lane| lane.chat == bot);
    if !own_chat || hub.team.answering(bot) || hub.routines.running(bot) {
        let why = anyhow!("add_work works only in your own chat with the user: its report comes back there");
        return Err((Admission::Elsewhere, why));
    }
    let text = |key: &str| args[key].as_str().map(str::trim).filter(|t| !t.is_empty());
    let (title, brief) = checked(text("title"), text("brief"))?;
    let hops = hub.tasks.hops(bot) + 1;
    if hops > MAX_HOPS {
        crate::chat::team::limit::stopped(hub, bot, bot, crate::chat::team::limit::Via::Work);
        let why = anyhow!(
            "too much work was added in a row without the user: the user has a Continue button in the chat; tell them where things stand and end your turn"
        );
        return Err((Admission::Hops, why));
    }
    let folder = text("folder").map(checked_folder).transpose()?;
    let asked = match &args["place"] {
        Value::Null => None,
        place => Some(
            PlaceChoice::deserialize(place).map_err(|_| (Admission::Invalid, anyhow!("place is here or worktree")))?,
        ),
    };
    let mut item = Item::waiting(title, brief, users_words(hub, bot), text("after"), hops);
    item.setup.cwd = folder;
    let folder = item.existing_folder(&cfg).map_err(|error| (Admission::Invalid, error))?;
    let kind = FolderKind::of(&cfg, &folder);
    let (place, place_note) = placed(asked, kind, &folder, item.after.is_some());
    item.setup.place = place;
    let id = items::change(hub, bot, |items| Ok(place_item(items, item))).map_err(failed)??;
    hub.tasks.added.locked().entry(bot.to_owned()).or_default().push(id.clone());
    Ok(Admitted { id, folder: kind, place, place_note })
}

/// A folder `add_work` names: the full path of one that exists (any folder: the bot proposes its projects).
fn checked_folder(folder: &str) -> Result<String, Refusal> {
    if !crate::agent::workspace::is_existing_folder(folder) {
        return Err((Admission::Invalid, anyhow!("folder must be the full path of a folder that exists: {folder}")));
    }
    Ok(folder.to_owned())
}

/// The user's own typed message the bot's running turn answers.
fn users_words(hub: &Hub, bot: &str) -> Option<String> {
    let entry = hub.store.entry(&hub.runtime(bot).message?)?;
    entry.data["text"].as_str().map(str::trim).filter(|t| !t.is_empty()).map(str::to_owned)
}

/// Refuses an item the user started as a task: it is theirs, not the bot's to steer or write to.
pub(super) fn the_bots(hub: &Hub, bot: &str, id: &str) -> Result<()> {
    if items::load(&hub.store, bot).iter().any(|i| i.id == id && i.source.is_user()) {
        let why = "that task is one the user started: it's theirs to steer, not yours".to_owned();
        return Err(thread::Refused::error("users", why));
    }
    Ok(())
}

fn steer_item(hub: &Arc<Hub>, bot: &str, args: &Value) -> Result<Value> {
    let text = |key: &str| args[key].as_str().map(str::trim).filter(|t| !t.is_empty());
    let id = text("item").ok_or_else(|| anyhow!("item is required: its id, from work_status"))?;
    the_bots(hub, bot, id)?;
    let refused = || anyhow!("how is first, now, replace or stop");
    let how = Steer::deserialize(&args["how"]).map_err(|_| refused())?;
    match how {
        Steer::First => steer::first(hub, bot, id)?,
        Steer::Now => steer::cut_in(hub, bot, id, text("insteadOf"))?,
        Steer::Replace => {
            let brief = text("brief").ok_or_else(|| anyhow!("replace needs the new brief"))?;
            if brief.len() > MAX_BRIEF {
                bail!("brief is at most {MAX_BRIEF} bytes");
            }
            steer::replace(hub, bot, id, brief)?;
        }
        Steer::Stop => steer::stop(hub, bot, Some(id))?,
        Steer::Drop | Steer::Retry => return Err(refused()),
    }
    // What the user said about it stays with it: a task that starts it again reads it.
    if let Some(words) = users_words(hub, bot) {
        items::change(hub, bot, |items| {
            if let Some(item) = items.iter_mut().find(|i| i.id == id && i.notes.last() != Some(&words)) {
                item.notes.push(words);
            }
            Ok(())
        })?;
    }
    let properties = json!({"how": how, "by": "bot"});
    analytics::capture(hub, Event::WorkSteered, properties.as_object().cloned().unwrap_or_default());
    let status = items::load(&hub.store, bot).iter().find(|i| i.id == id).map(|i| i.status);
    Ok(json!({"itemId": id, "status": status}))
}

fn tell(hub: &Arc<Hub>, bot: &str, args: &Value) -> Result<Value> {
    let now = args["now"] == true;
    let told = told(hub, bot, args, now);
    let properties = match &told {
        Ok((_, Told::Waiting)) => json!({"outcome": "told", "to": "waiting", "now": now}),
        Ok((_, Told::Working { .. })) => json!({"outcome": "told", "to": "working", "now": now}),
        Err(error) => {
            let category = error.downcast_ref::<thread::Refused>().map_or("other", |refused| refused.category);
            json!({"outcome": "failed", "error": category, "now": now})
        }
    };
    analytics::capture(hub, Event::WorkTold, properties.as_object().cloned().unwrap_or_default());
    let (id, to) = told?;
    let (status, note) = match to {
        Told::Waiting => (Status::Waiting, "It reads this when it starts."),
        Told::Working { queued: false } => (Status::Working, "Its task takes it now."),
        Told::Working { queued: true } if now => (Status::Working, "It takes this at its next step."),
        Told::Working { queued: true } => {
            (Status::Working, "It takes this once its current turn ends, before it reports.")
        }
    };
    Ok(json!({"itemId": id, "status": status, "note": note}))
}

/// Passes `tell_work`'s words on to the item its arguments name: that item's id, and where they went.
fn told(hub: &Arc<Hub>, bot: &str, args: &Value, now: bool) -> Result<(String, Told)> {
    let text = |key: &str| args[key].as_str().map(str::trim).filter(|t| !t.is_empty());
    let invalid = |why: String| thread::Refused::error("invalid", why);
    let id = text("item").ok_or_else(|| invalid("item is required: its id, from work_status".into()))?;
    let words = text("text").ok_or_else(|| invalid("text is required: what to pass on".into()))?;
    if words.len() > MAX_BRIEF {
        return Err(invalid(format!("text is at most {MAX_BRIEF} bytes")));
    }
    the_bots(hub, bot, id)?;
    Ok((id.to_owned(), thread::tell(hub, bot, id, words, now)?))
}

fn status(hub: &Hub, bot: &str) -> Result<Value> {
    let cfg = hub.store.bot(bot)?.ok_or_else(|| anyhow!("unknown bot"))?.config;
    let now = now_ms();
    let work: Vec<Value> = items::seen_by_bot(hub, &cfg)
        .into_iter()
        .map(|Shown { item, status, activity, wait }| {
            let seconds = item.started_at.map(|at| (item.ended_at.unwrap_or(now) - at) / 1000);
            json!({
                "itemId": item.id, "title": item.title, "status": status, "doingNow": activity, "wait": wait,
                "seconds": seconds, "result": item.result,
            })
        })
        .collect();
    Ok(json!({"work": work}))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn waiting(after: Option<&str>) -> Item {
        Item::waiting("Fix login", "x", None, after, 1)
    }

    #[test]
    fn a_box_takes_twenty_waiting_items_and_refuses_the_next() {
        let mut items = Vec::new();
        for n in 0..MAX_WAITING {
            place_item(&mut items, waiting(None)).unwrap_or_else(|(_, why)| panic!("item {n}: {why}"));
        }
        assert_eq!(items.last().map(|i| i.order), i64::try_from(MAX_WAITING).ok(), "each one last");
        let (outcome, why) = place_item(&mut items, waiting(None)).expect_err("the box is full");
        assert_eq!(outcome, Admission::Full);
        assert_eq!(why.to_string(), "20 items already wait: let some finish, or drop one, before adding more");

        // Only the waiting count: one that started leaves room for another.
        items[0].status = Status::Working;
        let first = items[0].id.clone();
        place_item(&mut items, waiting(Some(&first))).unwrap_or_else(|(_, why)| panic!("after a working item: {why}"));
        items[1].status = Status::Done;
        let ended = items[1].id.clone();
        let (outcome, _) = place_item(&mut items, waiting(Some(&ended))).expect_err("it follows an ended item");
        assert_eq!(outcome, Admission::Invalid);
    }

    #[test]
    fn an_items_place_says_why_when_it_cant_apply() {
        let repo = std::env::temp_dir().join(format!("codync-place-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(repo.join(".git")).expect("a repository");
        let (repo, plain) = (repo.to_string_lossy().into_owned(), std::env::temp_dir().to_string_lossy().into_owned());
        let worktree = Some(PlaceChoice::Worktree);
        assert_eq!(placed(worktree, FolderKind::Project, &repo, false), (worktree, None));
        let (left, why) = placed(worktree, FolderKind::Home, &repo, false);
        assert!(left.is_none() && why.is_some_and(|w| w.contains("own folder")), "the bot's own folder: never");
        let (left, why) = placed(worktree, FolderKind::Other, &plain, false);
        assert!(left.is_none() && why.is_some_and(|w| w.contains("git repository")), "outside a repository");
        assert!(placed(Some(PlaceChoice::Here), FolderKind::Project, &repo, true).1.is_some(), "after: where it was");
        assert_eq!(placed(None, FolderKind::Project, &repo, false), (None, None), "the mode decides");
        let _ = std::fs::remove_dir_all(&repo);
    }

    #[test]
    fn a_title_or_a_brief_too_long_is_refused() {
        let (title, brief) = ("t".repeat(MAX_TITLE), "b".repeat(MAX_BRIEF));
        assert!(checked(Some(&title), Some(&brief)).is_ok(), "at the limits");
        let long_title = format!("{title}t");
        let long_brief = format!("{brief}b");
        for (title, brief) in [(long_title.as_str(), "b"), ("t", long_brief.as_str())] {
            let (outcome, why) = checked(Some(title), Some(brief)).expect_err("too long");
            assert_eq!(outcome, Admission::Invalid);
            assert_eq!(why.to_string(), "title is at most 80 characters and brief at most 32000 bytes");
        }
        let (outcome, _) = checked(Some("t"), None).expect_err("no brief");
        assert_eq!(outcome, Admission::Invalid);
    }

    #[test]
    fn tell_work_notes_a_waiting_item_refuses_an_ended_one_or_anothers_and_is_the_chats_alone() {
        use crate::chat::calls::context::tests::{bot, hub};
        let hub = hub();
        hub.store.save_bot(&bot("b1", "Mika", "/tmp")).expect("saved");
        let items = json!([
            {"id": "i1", "title": "Docs", "brief": "x", "status": "waiting", "order": 1, "createdAt": 0},
            {"id": "i2", "title": "Login", "brief": "x", "status": "done", "order": 0, "createdAt": 0, "endedAt": 1},
            {"id": "i3", "title": "Morning", "brief": "x", "status": "waiting", "order": 2, "createdAt": 0,
                "source": {"kind": "routine", "id": "r", "run": "u"}},
        ]);
        hub.store.kv_set(&format!("{}b1", items::PREFIX), &items.to_string()).expect("saved");

        let told =
            call_now(&hub, "b1", "tell_work", &json!({"item": "i1", "text": " also the API page "})).expect("told");
        assert_eq!(
            (told["status"].as_str(), told["note"].as_str()),
            (Some("waiting"), Some("It reads this when it starts."))
        );
        let docs = items::load(&hub.store, "b1").remove(0);
        assert_eq!(docs.relayed, ["also the API page"], "its task reads it when it starts");
        assert!(docs.notes.is_empty(), "never as the user's own words");

        let category = |error: anyhow::Error| error.downcast_ref::<thread::Refused>().map(|refused| refused.category);
        let ended = call_now(&hub, "b1", "tell_work", &json!({"item": "i2", "text": "more"})).expect_err("it ended");
        assert!(ended.to_string().starts_with("\"Login\" has ended"), "{ended}");
        assert_eq!(category(ended), Some("ended"));
        let run = call_now(&hub, "b1", "tell_work", &json!({"item": "i3", "text": "skip staging"})).expect_err("a run");
        assert_eq!(run.to_string(), "\"Morning\" is a routine's run: it can be read and stopped, not written to");
        assert_eq!(category(run), Some("read_only"));
        assert!(items::load(&hub.store, "b1")[2].relayed.is_empty(), "nothing it would never read");
        let empty = call_now(&hub, "b1", "tell_work", &json!({"item": "i1"})).expect_err("nothing to pass on");
        assert_eq!(category(empty), Some("invalid"));

        let task = crate::store::BotConfig { task_of: Some("b1".into()), ..bot("task-1", "Mika", "/tmp") };
        hub.store.save_bot(&task).expect("saved");
        assert!(refuse_chat_only(&hub, "task-1", "tell_work").is_err(), "a task can't tell its siblings");
        hub.ceilings.set("b1", codync_network::Access::Ask);
        assert!(hub.ceilings.check_tool("b1", "tell_work").is_err(), "it acts later: held under a ceiling");
    }

    #[test]
    fn the_users_own_task_is_neither_seen_nor_steered_by_the_bot() {
        use crate::chat::calls::context::tests::{bot, hub};
        let hub = hub();
        hub.store.save_bot(&bot("b1", "Mika", "/tmp")).expect("saved");
        let items = json!([
            {"id": "i1", "title": "Docs", "brief": "x", "status": "waiting", "order": 1, "createdAt": 0},
            {"id": "u1", "title": "Mine", "brief": "x", "status": "waiting", "order": 2, "createdAt": 0,
                "source": {"kind": "user"}},
        ]);
        hub.store.kv_set(&format!("{}b1", items::PREFIX), &items.to_string()).expect("saved");

        let category = |error: anyhow::Error| error.downcast_ref::<thread::Refused>().map(|refused| refused.category);
        let told = call_now(&hub, "b1", "tell_work", &json!({"item": "u1", "text": "x"})).expect_err("the user's");
        assert_eq!(category(told), Some("users"));
        let steered =
            call_now(&hub, "b1", "steer_work", &json!({"item": "u1", "how": "first"})).expect_err("the user's");
        assert_eq!(category(steered), Some("users"));
        let listed = call_now(&hub, "b1", "work_status", &json!({})).expect("listed");
        assert_eq!(
            listed["work"].as_array().map(|w| w.iter().map(|i| i["itemId"].clone()).collect()),
            Some(vec![json!("i1")])
        );
        let recalled = call_now(&hub, "b1", "recall_work", &json!({})).expect("recalled");
        assert!(!recalled.to_string().contains("u1"), "{recalled}");
        let cfg = hub.store.bot("b1").expect("read").expect("there").config;
        let lines = items::lines(&hub, &cfg, "Your").expect("one live");
        assert!(lines.contains("Docs") && !lines.contains("Mine"), "{lines}");
    }

    /// #386: work added too often in a row leaves the user one Continue notice a turn, as the handoff limit does.
    #[tokio::test]
    async fn work_past_the_limit_leaves_the_user_one_continue_notice() {
        let dir = std::env::temp_dir().join(format!("codync-work-limit-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = crate::store::Store::open(&dir.join("t.db")).unwrap();
        store
            .save_bot(&serde_json::from_value(json!({"id": "bot", "name": "B", "backend": "claude"})).unwrap())
            .unwrap();
        let identity = crate::remote::identity::Identity::load_or_create(&dir).unwrap();
        let hub = Hub::new(store, "test".into(), identity, "test".into(), 0);
        hub.set_runtime("bot", |r| r.lane = Some(crate::store::Lane::main("bot")));
        hub.tasks.answering("bot", MAX_HOPS);
        for _ in 0..2 {
            let (outcome, _) =
                admit(&hub, "bot", &json!({"title": "Dig", "brief": "Look"})).expect_err("past the limit");
            assert_eq!(outcome, Admission::Hops);
        }
        let notices = hub.store.history("bot", i64::MAX, 100).unwrap();
        let continues = notices.iter().filter(|e| e.kind == "notice" && e.data["continueChain"] == true).count();
        assert_eq!(continues, 1, "one notice a turn, however often the bot tries");
        let _ = std::fs::remove_dir_all(dir);
    }
}
