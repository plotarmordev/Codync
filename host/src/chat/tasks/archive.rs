//! Older tasks ([older tasks](../../../../docs/features/background-tasks.md#older-tasks)): a box is read whole on every
//! change, so an item leaves it a week after it ended ([`super::KEEP_MS`]) for the archive, and its task bot, once no
//! item in the box names it, is parked out of `bots`: the transcript, session and folder stay, so the task still
//! opens, and a message reopens it as before. Nothing is deleted until the user says so: `threadKeepDays` (kv, none
//! for forever, the default) deletes a task that many days after it last ended, with its transcript and own folder.

use super::items::{self, Item};
use crate::analytics::errors::host_error;
use crate::analytics::{self, Event};
use crate::hub::Hub;
use crate::store::{Archived, BotConfig, Store, now_ms};
use anyhow::{Result, ensure};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;

/// The kv key of how many days after it last ended a task is deleted; none: never.
const KEEP_DAYS: &str = "threadKeepDays";
/// The most days the user may pick: about ten years.
const MOST_DAYS: u32 = 3650;
const DAY_MS: i64 = 24 * 60 * 60 * 1000;
/// How often the archive is looked over while the host runs, besides at its start and when the choice changes.
const EVERY: Duration = Duration::from_hours(24);
/// Older tasks one page holds at most.
const PAGE: usize = 30;
/// Where items went before the archive (`recall_work`'s past): moved in once.
const OLD_PAST: &str = "work-past.";

/// How many days after it last ended a task is deleted; 0: never.
pub fn keep_days(store: &Store) -> u32 {
    store.kv_get(KEEP_DAYS).and_then(|days| days.parse().ok()).unwrap_or(0)
}

/// The user's choice: 0 keeps tasks for good, else a week to [`MOST_DAYS`] (an item is in its box its first week).
/// What is older goes now; then apps connected hear a `threadKeepDays` event, the others in their next `hello`.
pub fn set_keep_days(hub: &Arc<Hub>, days: u32) -> Result<()> {
    ensure!(days == 0 || (7..=MOST_DAYS).contains(&days), "keep tasks for good (0) or 7 to {MOST_DAYS} days");
    if days == 0 { hub.store.kv_delete(KEEP_DAYS) } else { hub.store.kv_set(KEEP_DAYS, &days.to_string()) }?;
    delete_old(hub);
    announce(hub);
    Ok(())
}

/// The `threadKeepDays` event: apps list their older tasks again, without the ones just deleted.
fn announce(hub: &Hub) {
    let _ = hub.events.send(json!({"type": "threadKeepDays", "days": keep_days(&hub.store)}));
}

/// `old`, items leaving `bot`'s box (`left`: what stays in it), go to the archive, and the task bots no item in the
/// box names any more are parked. Refused when the archive can't keep them: they stay in the box.
pub(super) fn put(hub: &Hub, bot: &str, old: &[Item], left: &[Item]) -> Result<()> {
    if old.is_empty() {
        return Ok(());
    }
    let rows: Vec<_> = old
        .iter()
        .filter_map(|item| {
            let data = serde_json::to_string(&Item { report: None, ..item.clone() }).ok()?;
            Some((item.id.clone(), item.task.clone(), item.ended_at.unwrap_or_else(now_ms), data))
        })
        .collect();
    hub.store.archive_items(bot, &rows)?;
    let named: BTreeSet<&str> = left.iter().filter_map(|i| i.task.as_deref()).collect();
    let tasks: BTreeSet<&str> = old.iter().filter_map(|i| i.task.as_deref()).filter(|t| !named.contains(t)).collect();
    for task in tasks {
        let Ok(Some(row)) = hub.store.bot(task) else { continue };
        if row.deleted {
            continue;
        }
        hub.stop_actor(task);
        if let Err(error) = hub.store.park_task(&row.config) {
            tracing::warn!(task, error = format!("{error:#}"), "couldn't park a task");
        }
    }
    Ok(())
}

/// `bot`'s item `id` back in its box from the archive, with its task, for the user to write to it, try it again or
/// remove it. Nothing when it is in the box.
pub fn bring_back(hub: &Hub, bot: &str, id: &str) -> Result<()> {
    if items::load(&hub.store, bot).iter().any(|i| i.id == id) {
        return Ok(());
    }
    // Under the box's lock: the dispatcher never sees the task back without the item that names it, and the item
    // leaves the archive only on its way into the box.
    items::change(hub, bot, |items| {
        if items.iter().any(|i| i.id == id) {
            return Ok(());
        }
        let Some(data) = hub.store.archived_item(bot, id)? else { return Ok(()) };
        let item: Item = serde_json::from_str(&data)?;
        if let Some(task) = &item.task {
            hub.store.unpark_task(task)?;
        }
        hub.store.unarchive_item(bot, id)?;
        items.push(item);
        Ok(())
    })
}

/// An archived item as kept; one that can't be read is left out, and said.
fn read(data: &str) -> Option<Item> {
    serde_json::from_str(data)
        .inspect_err(|error| tracing::warn!(error = format!("{error:#}"), "couldn't read an archived item"))
        .ok()
}

/// Task bot `id` of `bot`'s, parked or not.
pub(super) fn task(store: &Store, bot: &str, id: &str) -> Option<BotConfig> {
    let row = store.bot(id).ok().flatten().filter(|r| !r.deleted).map(|r| r.config);
    row.or_else(|| store.parked_task(id).ok().flatten()).filter(|c| c.task_of.as_deref() == Some(bot))
}

/// The archived item `task` worked last, of `bot`'s.
pub(super) fn last_of(store: &Store, bot: &str, task: &str) -> Option<Item> {
    let all = store.archived_by_task(bot, task).ok()?;
    all.iter().find_map(|data| read(data))
}

/// `bot`'s archived items that ended at `since` or later, newest first, `most` at most (`recall_work`).
pub(super) fn recent(store: &Store, bot: &str, since: i64, most: usize) -> Vec<Item> {
    let all = store.archived_of(bot, since, i64::try_from(most).unwrap_or(i64::MAX)).unwrap_or_default();
    all.iter().filter_map(|data| read(data)).collect()
}

/// A deleted bot's archive goes with it, and its parked tasks with their transcripts.
pub(super) fn forget(hub: &Hub, bot: &str) {
    let old: Vec<Item> = match hub.store.drop_archived_of(bot) {
        Ok(all) => all.iter().filter_map(|data| read(data)).collect(),
        Err(error) => {
            tracing::warn!(bot, error = format!("{error:#}"), "couldn't drop a bot's archive");
            Vec::new()
        }
    };
    super::plain::forget_folders(&old);
    for task in hub.store.parked_of(bot).unwrap_or_default() {
        delete_parked(hub, &task);
    }
}

/// Parked task `id` goes for good, as a task bot does (`Hub::delete_bot`).
fn delete_parked(hub: &Hub, id: &str) {
    let deleted = hub.store.unpark_task(id).and_then(|_| hub.delete_bot(id));
    if let Err(error) = deleted {
        tracing::warn!(task = id, error = format!("{error:#}"), "couldn't remove an old task");
    }
}

/// The tasks that last ended longer ago than the user keeps them go, and apps connected list theirs again.
pub fn prune(hub: &Hub) {
    if delete_old(hub) {
        announce(hub);
    }
}

/// The tasks that last ended longer ago than the user keeps them go, with their transcripts and own folders;
/// whether any went.
fn delete_old(hub: &Hub) -> bool {
    let days = keep_days(&hub.store);
    if days == 0 {
        return false;
    }
    let before = now_ms() - i64::from(days) * DAY_MS;
    let old = match hub.store.drop_archived_before(before) {
        Ok(old) => old,
        Err(error) => {
            host_error!(hub, "couldn't delete old tasks", error = format!("{error:#}"));
            return false;
        }
    };
    if old.is_empty() {
        return false;
    }
    let items: Vec<Item> = old.iter().filter_map(|a| read(&a.data)).collect();
    super::plain::forget_folders(&items);
    super::worktree::leave(&items);
    let tasks: BTreeSet<&str> = items.iter().filter_map(|i| i.task.as_deref()).collect();
    for task in tasks {
        // A task that worked a later item stays with it.
        if !hub.store.task_archived(task).unwrap_or(true) && hub.store.bot(task).ok().flatten().is_none() {
            delete_parked(hub, task);
        }
    }
    let count = items.len();
    tracing::info!(count, days, "deleted old tasks");
    let properties = json!({"count": count, "days": days});
    analytics::capture(hub, Event::ThreadsDeleted, properties.as_object().cloned().unwrap_or_default());
    true
}

/// `olderThreads`: the ended tasks of `only` bot, or of every bot on the computer, that ended before `before` (none:
/// the first page), newest first, [`PAGE`] at a time but never splitting the ones that ended at once; the wire carries
/// only the last day's. Each is its bot's `work` row with `botId`.
pub fn older(hub: &Hub, only: Option<&str>, before: Option<i64>) -> Result<Value> {
    let first = before.is_none();
    let before = before.unwrap_or(i64::MAX);
    let lives = |bot: &str| hub.store.bot(bot).ok().flatten().is_some_and(|r| !r.deleted);
    let mut found: Vec<(String, Item)> = items::bots(&hub.store)
        .into_iter()
        .filter(|bot| only.is_none_or(|only| only == bot) && lives(bot))
        .flat_map(|bot| {
            let ended = items::load(&hub.store, &bot).into_iter().filter(|i| i.status.ended());
            ended.map(move |item| (bot.clone(), item)).collect::<Vec<_>>()
        })
        .filter(|(_, item)| item.ended_at.is_some_and(|at| at < before))
        .collect();
    let page = i64::try_from(PAGE + 1).unwrap_or(i64::MAX);
    let add = |archived: Vec<Archived>, found: &mut Vec<(String, Item)>| {
        for a in archived {
            if let Some(item) = read(&a.data)
                && lives(&a.bot)
                && !found.iter().any(|(bot, i)| *bot == a.bot && i.id == item.id)
            {
                found.push((a.bot, item));
            }
        }
    };
    add(hub.store.archived_before(only, before, page)?, &mut found);
    found.sort_by_key(|(_, item)| std::cmp::Reverse(item.ended_at));
    let more = found.len() > PAGE;
    // The next page asks before the last row's end: every other that ended then comes on this one.
    if let Some(edge) = found.get(PAGE.saturating_sub(1)).and_then(|(_, i)| i.ended_at)
        && more
    {
        found.retain(|(_, i)| i.ended_at >= Some(edge));
        add(hub.store.archived_at(only, edge)?, &mut found);
    } else {
        found.truncate(PAGE);
    }
    let count = found.len();
    let tasks: Vec<Value> = found
        .into_iter()
        .map(|(bot, item)| {
            let task = item.task.as_deref().and_then(|id| task(&hub.store, &bot, id));
            let runs_on = task.as_ref().map(|c| items::agent_of(&hub.store, c));
            let status = item.status;
            let mut row = items::row(item, status, "", None, false, task.as_ref().map(|c| &c.cwd), runs_on);
            row["botId"] = bot.into();
            row
        })
        .collect();
    let properties = json!({"count": count, "more": more, "first": first});
    analytics::capture(hub, Event::OlderThreadsListed, properties.as_object().cloned().unwrap_or_default());
    Ok(json!({"threads": tasks, "more": more}))
}

/// Every box's items that ended over a week ago go to the archive, then what the user keeps no longer goes; once a
/// day for as long as the host runs (host start does both itself).
pub fn look_daily(hub: &Arc<Hub>) {
    let Ok(runtime) = tokio::runtime::Handle::try_current() else { return };
    let hub = Arc::downgrade(hub);
    runtime.spawn(async move {
        loop {
            tokio::time::sleep(EVERY).await;
            let Some(hub) = hub.upgrade() else { return };
            let looked = tokio::task::spawn_blocking(move || {
                for bot in items::bots(&hub.store) {
                    super::age(&hub, &bot);
                }
                prune(&hub);
            });
            if let Err(error) = looked.await {
                tracing::warn!(error = %error, "the daily look at old tasks failed");
            }
        }
    });
}

/// The items a host from before the archive kept for `recall_work` (`work-past.<bot>`) move into it, once.
pub(super) fn move_old_past(hub: &Hub) {
    for key in hub.store.kv_prefix(OLD_PAST) {
        let Some(bot) = key.strip_prefix(OLD_PAST) else { continue };
        let old: Vec<Item> =
            hub.store.kv_get(&key).and_then(|json| serde_json::from_str(&json).ok()).unwrap_or_default();
        // The box keeps a task an item in it still names.
        let left = items::load(&hub.store, bot);
        if let Err(error) = put(hub, bot, &old, &left).and_then(|()| hub.store.kv_delete(&key)) {
            tracing::warn!(bot, error = format!("{error:#}"), "couldn't move a bot's past items");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::calls::context::tests::{bot, hub};
    use crate::store::{EntryKind, Lane};

    const TASK: &str = "task-old";

    /// Bot `b1` with one task the user started, ended `days` ago, whose transcript holds one step.
    fn ended_task(days: i64) -> Arc<Hub> {
        let hub = hub();
        hub.store.save_bot(&bot("b1", "Mika", "/tmp")).expect("saved");
        let task = BotConfig { task_of: Some("b1".into()), ..bot(TASK, "Mika", "/tmp/work") };
        hub.store.save_bot(&task).expect("saved");
        let step = json!({"text": "all done"});
        hub.store.insert_entry(&Lane::main(TASK), EntryKind::Agent, 1, &step).expect("a step");
        let items = json!([{"id": "i1", "title": "Fix login", "brief": "x", "status": "done", "order": 0,
            "createdAt": 0, "endedAt": now_ms() - days * DAY_MS, "task": TASK, "source": {"kind": "user"}}]);
        hub.store.kv_set("work.b1", &items.to_string()).expect("a box");
        hub
    }

    #[test]
    fn a_task_ended_weeks_ago_leaves_the_box_and_still_opens_and_lists() {
        let hub = ended_task(10);
        super::super::age(&hub, "b1");
        assert_eq!(items::load(&hub.store, "b1").len(), 0, "out of the box");
        assert!(hub.store.bot(TASK).expect("read").is_none(), "parked out of the bots");
        let opened = super::super::entries(&hub, "b1", TASK, 500).expect("it opens");
        assert_eq!(
            (opened["task"]["title"].as_str(), opened["entries"].as_array().map(Vec::len)),
            (Some("Fix login"), Some(1))
        );
        let page = older(&hub, None, None).expect("a page");
        let row = &page["threads"][0];
        assert_eq!(
            (
                row["botId"].as_str(),
                row["id"].as_str(),
                row["talk"].as_bool(),
                row["folder"].as_str(),
                row["runsOn"].is_string()
            ),
            (Some("b1"), Some("i1"), Some(true), Some("/tmp/work"), true)
        );
        assert_eq!(page["more"], false);
        let earlier = older(&hub, Some("b1"), row["endedAt"].as_i64()).expect("the next page");
        assert_eq!(earlier["threads"], json!([]));
    }

    #[test]
    fn an_archived_item_comes_back_whole_to_be_written_to() {
        let hub = ended_task(10);
        super::super::age(&hub, "b1");
        bring_back(&hub, "b1", "i1").expect("back");
        assert_eq!(items::load(&hub.store, "b1").iter().map(|i| i.id.as_str()).collect::<Vec<_>>(), ["i1"]);
        assert!(hub.store.bot(TASK).expect("read").is_some_and(|r| !r.deleted), "its task bot is back");
        assert!(
            older(&hub, None, None).expect("a page")["threads"].as_array().is_some_and(|t| t.len() == 1),
            "listed once"
        );
    }

    #[tokio::test]
    async fn old_tasks_stay_for_good_unless_the_user_picks_days() {
        let hub = ended_task(40);
        super::super::age(&hub, "b1");
        prune(&hub);
        assert_eq!(keep_days(&hub.store), 0);
        assert!(super::super::entries(&hub, "b1", TASK, 500).is_ok(), "kept for good by default");
        assert!(set_keep_days(&hub, 3).is_err() && set_keep_days(&hub, 4000).is_err(), "a week to ten years");
        set_keep_days(&hub, 60).expect("set");
        assert!(super::super::entries(&hub, "b1", TASK, 500).is_ok(), "40 days is within 60");
        set_keep_days(&hub, 30).expect("set");
        assert!(super::super::entries(&hub, "b1", TASK, 500).is_err(), "gone");
        assert_eq!(hub.store.history(TASK, i64::MAX, 10).expect("read").len(), 0, "with its transcript");
        assert!(hub.store.parked_task(TASK).expect("read").is_none());
        set_keep_days(&hub, 0).expect("forever again");
        assert_eq!(keep_days(&hub.store), 0);
    }

    #[tokio::test]
    async fn the_daily_look_tells_apps_when_it_deleted_tasks() {
        let hub = ended_task(40);
        super::super::age(&hub, "b1");
        hub.store.kv_set(KEEP_DAYS, "30").expect("kept 30 days");
        let mut events = hub.events.subscribe();
        let mut told = || {
            std::iter::from_fn(|| events.try_recv().ok()).filter(|e| e["type"] == "threadKeepDays").collect::<Vec<_>>()
        };
        prune(&hub);
        assert_eq!(told(), [json!({"type": "threadKeepDays", "days": 30})]);
        prune(&hub);
        assert!(told().is_empty(), "nothing deleted, nothing said");
    }

    #[test]
    fn a_task_back_from_the_archive_goes_on_in_its_session() {
        let hub = ended_task(10);
        hub.store.set_session(TASK, Some("s1")).expect("a session");
        super::super::age(&hub, "b1");
        bring_back(&hub, "b1", "i1").expect("back");
        let row = hub.store.bot(TASK).expect("read").expect("its task bot");
        assert_eq!(row.session_id.as_deref(), Some("s1"));
        assert!(hub.store.archived_item("b1", "i1").expect("read").is_none(), "out of the archive");
    }

    #[tokio::test]
    async fn an_archived_item_can_be_removed() {
        let hub = ended_task(10);
        super::super::age(&hub, "b1");
        super::super::drop_item(&hub, "b1", "i1").expect("dropped");
        assert_eq!(items::load(&hub.store, "b1").len(), 0);
        assert!(hub.store.archived_item("b1", "i1").expect("read").is_none());
        assert_eq!(older(&hub, None, None).expect("a page")["threads"], json!([]));
    }

    #[test]
    fn a_page_never_splits_tasks_that_ended_at_once() {
        let hub = hub();
        hub.store.save_bot(&bot("b1", "Mika", "/tmp")).expect("saved");
        let at = now_ms() - 10 * DAY_MS;
        let ended: Vec<Value> = (0..PAGE + 2)
            .map(|n| {
                json!({"id": format!("i{n}"), "title": "x", "brief": "x", "status": "stopped", "order": 0,
                "createdAt": 0, "endedAt": at, "source": {"kind": "user"}})
            })
            .collect();
        hub.store.kv_set("work.b1", &Value::from(ended).to_string()).expect("a box");
        super::super::age(&hub, "b1");
        let page = older(&hub, None, None).expect("a page");
        assert_eq!(page["threads"].as_array().map(Vec::len), Some(PAGE + 2), "every one that ended then");
        assert_eq!(older(&hub, None, Some(at)).expect("the next page")["threads"], json!([]));
    }

    #[test]
    fn past_items_of_an_older_host_move_in_and_only_those_that_still_open_list() {
        let hub = ended_task(1);
        let past = |id: &str, task: &str| {
            json!({"id": id, "title": id, "brief": "x", "status": "done", "order": 0, "createdAt": 0,
                "endedAt": now_ms() - 10 * DAY_MS, "task": task, "source": {"kind": "user"}})
        };
        let old = json!([past("p1", TASK), past("p2", "task-gone")]);
        hub.store.kv_set(&format!("{OLD_PAST}b1"), &old.to_string()).expect("past items");
        move_old_past(&hub);
        assert!(hub.store.bot(TASK).expect("read").is_some(), "the box's item still names its task");
        let page = older(&hub, None, None).expect("a page");
        let ids: Vec<&str> =
            page["threads"].as_array().into_iter().flatten().filter_map(|t| t["id"].as_str()).collect();
        assert_eq!(ids, ["i1", "p1"], "the box's own, then the one whose task is still here");
    }

    #[tokio::test]
    async fn a_deleted_bots_old_tasks_go_with_it() {
        let hub = ended_task(10);
        super::super::age(&hub, "b1");
        hub.delete_bot("b1").expect("deleted");
        assert!(hub.store.parked_task(TASK).expect("read").is_none());
        assert_eq!(hub.store.archived_before(None, i64::MAX, 10).expect("read").len(), 0);
        assert_eq!(hub.store.history(TASK, i64::MAX, 10).expect("read").len(), 0);
    }
}
