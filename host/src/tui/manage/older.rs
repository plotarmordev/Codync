//! A bot's tasks that ended before its `work` carries them (`olderThreads`), listed under Recent on its Details page
//! (`work.rs` lays them out), a page at a time as the cursor reaches the last row.

use serde_json::{Value, json};

use super::super::app::{App, Overlay, WorkItem};
use super::{DetailsPage, Reply};

/// A bot's older tasks, as far as they were paged.
#[derive(Default)]
pub struct OlderThreads {
    /// Newest first.
    pub rows: Vec<WorkItem>,
    /// The host has more; the next page comes when the cursor reaches the last row.
    pub more: bool,
    /// A page is on its way.
    pub loading: bool,
}

impl OlderThreads {
    /// A page's rows after these, each once.
    fn add(&mut self, page: &[Value]) {
        for item in page.iter().filter_map(WorkItem::parse) {
            if !self.rows.iter().any(|o| o.id == item.id) {
                self.rows.push(item);
            }
        }
    }

    /// What the next page asks before: the oldest row's end.
    fn before(&self) -> Option<i64> {
        self.rows.iter().filter_map(|i| i.ended_at).min()
    }
}

impl App {
    /// The cursor reached the page's last row: the next page of older tasks comes, if the host has more.
    pub(super) fn more_older(&self, l: &mut DetailsPage, rows: usize) {
        if !l.older.more || l.older.loading || l.cursor + 1 < rows {
            return;
        }
        let Some(before) = l.older.before() else { return };
        l.older.loading = true;
        let body = json!({"botId": l.bot, "before": before});
        self.sheet("olderThreads", body, Reply::OlderThreads(l.bot.clone()));
    }

    /// A page of `bot`'s older tasks arrived (`None`: it couldn't come); the cursor stays on its row.
    pub(in crate::tui) fn older_threads(&mut self, bot: &str, page: Option<&Value>) {
        let (work, max_agents) = self.bots.get(bot).map_or((&[][..], None), |b| (&b.work, b.max_agents));
        for o in &mut self.overlays {
            let Overlay::Details(l) = o else { continue };
            if l.bot != bot {
                continue;
            }
            let picked = l.picked(work, max_agents).map(|row| row.id().to_owned());
            l.older.loading = false;
            l.older.more = page.is_some_and(|p| p["more"] == true);
            l.older.add(page.and_then(|p| p["threads"].as_array()).map_or(&[][..], Vec::as_slice));
            l.keep_cursor(picked.as_deref(), work, max_agents);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::app::test_app;
    use super::super::DetailRow;
    use super::super::work::tests::bot_with;
    use super::*;

    fn task(id: &str, ended_at: i64) -> Value {
        json!({"id": id, "title": id, "status": "done", "task": format!("task-{id}"), "endedAt": ended_at})
    }

    #[test]
    fn older_pages_follow_the_bots_recent_tasks_once_each() {
        let mut app = test_app();
        bot_with(&mut app, &[("w", "done")]);
        app.overlays.push(Overlay::Details(DetailsPage { bot: "a".into(), ..Default::default() }));
        let first = json!({"threads": [task("w", 300), task("o1", 200)], "more": true});
        app.older_threads("a", Some(&first));
        app.older_threads("a", Some(&json!({"threads": [task("o1", 200), task("o2", 100)], "more": false})));
        let Some(Overlay::Details(l)) = app.overlays.last() else { panic!("the page is open") };
        let work = &app.bots["a"].work;
        let recent: Vec<&str> = l
            .sections(work, None)
            .into_iter()
            .filter(|s| s.title == "Recent")
            .flat_map(|s| s.rows)
            .filter_map(|r| if let DetailRow::Recent(i) = r { Some(i.id.as_str()) } else { None })
            .collect();
        assert_eq!(recent, ["w", "o1", "o2"], "the bot's own row first, then the older ones");
        assert_eq!((l.older.before(), l.older.more, l.older.loading), (Some(100), false, false));
    }
}
