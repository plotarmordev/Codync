//! A bot's box: its items (kv `work.<bot>`, one JSON list), how they read on the wire and to the bot itself.

use super::dispatch::{self, Standing, Wait};
use super::worktree::{Left, Place, PlaceChoice, Worktree};
use super::{Failure, Report};
use crate::LockExt;
use crate::hub::{BotStatus, Hub};
use crate::store::{BotConfig, Permission, Store, now_ms};
use anyhow::{Result, anyhow, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;

/// An ended item shows on the wire and in `work_status` this long.
const RECENT_MS: i64 = 24 * 60 * 60 * 1000;
/// Ended items the wire carries at most.
const RECENT_MOST: usize = 20;
/// How much of an item's result the wire carries.
const WIRE_RESULT: usize = 280;

/// Where an item is: the only words for work, on the wire, in every client and in the bot's tools.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "camelCase")]
pub enum Status {
    /// In the box; no task works on it.
    Waiting,
    Working,
    /// Its task waits on an approval. Never saved: a working item reads so while its task's card is open.
    NeedsYou,
    Done,
    Failed,
    Stopped,
}

impl Status {
    /// A task has it: working, or waiting on the user's approval.
    pub fn live(self) -> bool {
        matches!(self, Self::Working | Self::NeedsYou)
    }

    pub fn ended(self) -> bool {
        matches!(self, Self::Done | Self::Failed | Self::Stopped)
    }
}

/// Where an item came from (`work[].source`, absent for the bot's own).
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug, Default)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Source {
    /// The bot added it (`add_work`).
    #[default]
    Bot,
    /// The user started it themselves, as a thread with the bot (`startThread`): what it comes to is read in that
    /// thread, and never becomes a turn in the chat.
    User,
    /// A run of one of the bot's routines: `id` is the routine.
    Routine { id: String, run: String },
    /// Another bot's `message_bot` request: `id` is the asking bot, when it is one of this computer's.
    Request {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
    },
}

impl Source {
    pub fn is_bot(&self) -> bool {
        *self == Self::Bot
    }

    /// A thread the user started: theirs, so the bot doesn't see or steer it.
    pub fn is_user(&self) -> bool {
        *self == Self::User
    }

    /// It takes one of the bot's seats under `maxAgents` while it works: everything but a thread the user started.
    pub fn seated(&self) -> bool {
        !self.is_user()
    }

    /// The bot's own work, whoever added it (the bot, or the user as a thread): the user may steer it, try it again
    /// and write to it, where a routine's run or another bot's request has an owner elsewhere.
    pub fn own(&self) -> bool {
        matches!(self, Self::Bot | Self::User)
    }

    /// `work_added` and `work_finished`'s `source`.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Bot => "bot",
            Self::User => "user",
            Self::Routine { .. } => "routine",
            Self::Request { .. } => "request",
        }
    }

    /// `work[].source`: `{kind, id?}`; none for the bot's own (a thread the user started is one to every client), so a
    /// client that doesn't know sources shows the row.
    fn wire(&self) -> Option<Value> {
        match self {
            Self::Bot | Self::User => None,
            Self::Routine { id, .. } => Some(json!({"kind": self.kind(), "id": id})),
            Self::Request { id } => Some(match id {
                Some(id) => json!({"kind": self.kind(), "id": id}),
                None => json!({"kind": self.kind()}),
            }),
        }
    }
}

/// What a thread the user starts changes from its bot's setup, for its own task (`startThread`'s `setup`): the
/// agent (a thread without a bot names it) and its login, the folder it works in, its model and how it is approved.
/// What it leaves out is the bot's.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Setup {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backend: Option<String>,
    /// The agent's login profile; `""` its default login. None: the bot's (another agent's default login).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub login_profile: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<Model>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval: Option<Approval>,
    /// A thread opened beside a workflow to build it by talking (`BotConfig::workflow`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow: Option<String>,
    /// Where its task works, its folder or a worktree, when the item names it (`add_work`'s and `startThread`'s
    /// `place`); none: the bot's worktree mode decides (`worktree::wanted`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub place: Option<PlaceChoice>,
}

/// A model with its own settings (`BotConfig::model`, `model_options`): they go together, as in a bot's setup.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct Model {
    /// None: the agent's default.
    pub id: Option<String>,
    #[serde(default)]
    pub options: BTreeMap<String, String>,
}

/// How its requests are approved (`BotConfig::permission`, `mode`).
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct Approval {
    pub permission: Permission,
    /// None: the agent's default mode.
    pub mode: Option<String>,
}

impl Setup {
    pub fn is_bots(&self) -> bool {
        *self == Self::default()
    }

    /// `cfg` with what this changes.
    pub fn over(&self, cfg: &BotConfig) -> BotConfig {
        let mut cfg = cfg.clone();
        // Another agent runs on its own login, without the bot's fallbacks, command or Fast mode.
        if let Some(backend) = self.backend.as_ref().filter(|b| **b != cfg.backend) {
            backend.clone_into(&mut cfg.backend);
            cfg.login_profile = None;
            cfg.command = None;
            cfg.fast_mode = false;
            cfg.fallbacks.clear();
        }
        // A login the bot's chain falls back to is the thread's own now: that step goes.
        if let Some(profile) = &self.login_profile {
            cfg.login_profile = Some(profile.clone()).filter(|p| !p.is_empty());
            crate::agent::fallback::tidy(&mut cfg);
        }
        if let Some(cwd) = &self.cwd {
            cfg.cwd.clone_from(cwd);
        }
        if let Some(model) = &self.model {
            cfg.model.clone_from(&model.id);
            cfg.model_options.clone_from(&model.options);
        }
        if let Some(approval) = &self.approval {
            cfg.permission = approval.permission;
            cfg.mode.clone_from(&approval.mode);
        }
        if self.workflow.is_some() {
            cfg.workflow.clone_from(&self.workflow);
        }
        cfg
    }
}

/// One piece of work in a bot's box.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
#[expect(clippy::struct_excessive_bools, reason = "independent facts about an item")]
pub(super) struct Item {
    pub id: String,
    pub title: String,
    /// What its task is told: the bot's brief, a routine run's prompt (written when it starts) or the request.
    pub brief: String,
    #[serde(default, skip_serializing_if = "Source::is_bot")]
    pub source: Source,
    /// What its task runs with in place of its bot's setup.
    #[serde(default, skip_serializing_if = "Setup::is_bots")]
    pub setup: Setup,
    /// The user's own words that led to it, word for word.
    #[serde(default)]
    pub words: Option<String>,
    /// What the user said about it since, word for word.
    #[serde(default)]
    pub notes: Vec<String>,
    /// What the bot's chat passed on to it since (`tell_work`), word for word: never the user's own.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relayed: Vec<String>,
    pub status: Status,
    /// Its place among the waiting, lowest first.
    pub order: i64,
    /// An item it follows, in that item's session.
    #[serde(default)]
    pub after: Option<String>,
    /// The task bot that has or had it.
    #[serde(default)]
    pub task: Option<String>,
    /// It gave way to another item once, and starts again in a new session.
    #[serde(default)]
    pub cut_in: bool,
    /// A host restart already sent it back to waiting once: the next one fails it.
    #[serde(default)]
    pub restarted: bool,
    #[serde(default)]
    pub hops: u8,
    pub created_at: i64,
    #[serde(default)]
    pub started_at: Option<i64>,
    #[serde(default)]
    pub ended_at: Option<i64>,
    /// What its task reported, or why it didn't finish.
    #[serde(default)]
    pub result: Option<String>,
    /// Its report, until it has reached where it goes (its bot's turn for it has ended; a routine's result is in
    /// the chat; the asking bot has its reply): a host restart before that sends it again.
    #[serde(default)]
    pub report: Option<Report>,
    /// What kept it from finishing, with its report.
    #[serde(default)]
    pub failure: Option<Failure>,
    /// The host couldn't give it to its task: `work_finished` says `dispatch` where `failure` says `Start`. Only on
    /// the item as it ends, never saved.
    #[serde(skip)]
    pub undispatched: bool,
    /// Its task's session began on it (or on the item it follows, in the same session).
    #[serde(default)]
    pub began: bool,
    /// Where its new task works: in a git worktree of its own when another task of the bot worked in the
    /// repository as it began, or when it has one from before.
    #[serde(default)]
    pub place: Place,
    /// The worktree its task works or worked in (`worktree`).
    #[serde(default)]
    pub worktree: Option<Worktree>,
    /// What the user wrote to its task since its last report, word for word (`task`): its next report ends
    /// with it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub told: Vec<String>,
    /// What it reported before the user wrote to it again: a new session that picks the work up is told.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub said: Option<String>,
    /// The files the user started it with (`startThread`), as uploaded to its bot (`{id, name, size}`): its thread
    /// gets a copy of each as it starts.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<Value>,
}

/// Where an item's task works, as `work_added` says it (`folder`), never the path: the bot's own folder, one of
/// its projects, or another folder; for a thread without a bot, its own folder or another.
#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "camelCase")]
pub(super) enum FolderKind {
    Home,
    Project,
    Own,
    Other,
}

impl FolderKind {
    /// What `folder` is to bot `cfg`.
    pub(super) fn of(cfg: &BotConfig, folder: &str) -> Self {
        if cfg.plain {
            return if super::plain::is_own_folder(folder) { Self::Own } else { Self::Other };
        }
        if cfg.projects.iter().any(|p| p == folder) {
            Self::Project
        } else if folder == cfg.cwd {
            Self::Home
        } else {
            Self::Other
        }
    }
}

impl Item {
    /// The folder its new task works in (`cfg`: its bot): the one it names, else for the bot's own work and the
    /// user's the bot's default project, else the bot's own folder (a routine's run and another bot's request work
    /// where the bot lives).
    pub(super) fn folder(&self, cfg: &BotConfig) -> String {
        let project = self.source.own().then(|| crate::agent::workspace::default_project(cfg)).flatten();
        self.setup.cwd.as_deref().or(project).unwrap_or(&cfg.cwd).to_owned()
    }

    /// [`Self::folder`], refused once it is gone (a project unmounted or renamed since it was added): its task
    /// couldn't start there.
    pub(super) fn existing_folder(&self, cfg: &BotConfig) -> Result<String> {
        let folder = self.folder(cfg);
        ensure!(std::path::Path::new(&folder).is_dir(), "project folder does not exist: {folder}");
        Ok(folder)
    }

    /// Whether its task works in a git worktree of its own (one to come, or one it has), not in its folder.
    pub(super) fn in_worktree(&self) -> bool {
        self.place == Place::Worktree || self.worktree.as_ref().is_some_and(|w| w.left == Left::InUse)
    }

    /// A new item, waiting; its place is its box's to give.
    pub(super) fn waiting(title: &str, brief: &str, words: Option<String>, after: Option<&str>, hops: u8) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            title: title.to_owned(),
            brief: brief.to_owned(),
            source: Source::Bot,
            setup: Setup::default(),
            words,
            notes: Vec::new(),
            relayed: Vec::new(),
            status: Status::Waiting,
            order: 0,
            after: after.map(str::to_owned),
            task: None,
            cut_in: false,
            restarted: false,
            hops,
            created_at: now_ms(),
            started_at: None,
            ended_at: None,
            result: None,
            report: None,
            failure: None,
            undispatched: false,
            began: false,
            place: Place::Folder,
            worktree: None,
            told: Vec::new(),
            said: None,
            files: Vec::new(),
        }
    }

    /// Back to waiting at the front of `items`, to start again in a new task.
    pub(super) fn set_aside(&mut self, front: i64) {
        self.status = Status::Waiting;
        self.order = front;
        self.task = None;
        self.started_at = None;
        self.began = false;
    }
}

pub(super) const PREFIX: &str = "work.";

fn key(bot: &str) -> String {
    format!("{PREFIX}{bot}")
}

/// Every bot with a box.
pub(super) fn bots(store: &Store) -> Vec<String> {
    store.kv_prefix(PREFIX).iter().filter_map(|key| key.strip_prefix(PREFIX)).map(str::to_owned).collect()
}

pub(super) fn load(store: &Store, bot: &str) -> Vec<Item> {
    store.kv_get(&key(bot)).and_then(|json| serde_json::from_str(&json).ok()).unwrap_or_default()
}

/// Changes `bot`'s box. One change at a time, so two can't write over each other.
// ponytail: one lock for every bot's box; a lock per bot if boxes ever change often enough to wait on each other.
pub(super) fn change<T>(hub: &Hub, bot: &str, change: impl FnOnce(&mut Vec<Item>) -> Result<T>) -> Result<T> {
    let _g = hub.tasks.boxes.locked();
    let mut items = load(&hub.store, bot);
    let out = change(&mut items)?;
    hub.store.kv_set(&key(bot), &serde_json::to_string(&items)?)?;
    Ok(out)
}

/// A deleted bot's box goes with it.
pub(super) fn forget(store: &Store, bot: &str) {
    if let Err(error) = store.kv_delete(&key(bot)) {
        tracing::warn!(bot, error = format!("{error:#}"), "couldn't drop a bot's work");
    }
}

/// How long an ended item can still be recalled.
pub(super) const PAST_MS: i64 = 30 * 24 * 60 * 60 * 1000;
/// Items out of the box (`archive`) `recall_work` reads at most, the newest.
pub(super) const PAST_MOST: usize = 200;

/// Whether an item that ended `at` can still be recalled `now`.
pub(super) fn within_past(at: i64, now: i64) -> bool {
    now - at <= PAST_MS
}

/// An order before every waiting item's.
pub(super) fn front(items: &[Item]) -> i64 {
    items.iter().filter(|i| i.status == Status::Waiting).map(|i| i.order).min().unwrap_or(0) - 1
}

/// An order after every item's.
pub(super) fn last(items: &[Item]) -> i64 {
    items.iter().map(|i| i.order).max().unwrap_or(0) + 1
}

/// How many of `items` a task works on.
pub(super) fn working(items: &[Item]) -> usize {
    items.iter().filter(|i| i.status == Status::Working).count()
}

/// The item `id`.
pub(super) fn find<'a>(items: &'a mut [Item], id: &str) -> Result<&'a mut Item> {
    items.iter_mut().find(|i| i.id == id).ok_or_else(|| anyhow!("no such work"))
}

/// The item `task` works on.
pub(super) fn of_task<'a>(items: &'a [Item], task: &str) -> Option<&'a Item> {
    items.iter().find(|i| i.status == Status::Working && i.task.as_deref() == Some(task))
}

/// An item as its bot's clients and tools see it now: its status with its task's card, and what its task does.
pub(super) struct Shown {
    pub item: Item,
    pub status: Status,
    pub activity: String,
    pub wait: Option<Wait>,
}

/// `cfg`'s items worth showing: the working ones (oldest first), the waiting in dispatch order, then the ones ended
/// in the last day, newest first.
pub(super) fn shown(hub: &Hub, cfg: &BotConfig) -> Vec<Shown> {
    let items = load(&hub.store, &cfg.id);
    let mut waits: HashMap<String, Wait> = HashMap::new();
    if items.iter().any(|i| i.status == Status::Waiting) {
        let places = dispatch::places(hub, cfg, &items, None);
        let standings = dispatch::standings(
            &items,
            &places,
            |task| dispatch::task_state(hub, task, None),
            |item| dispatch::held(hub, item),
        );
        waits.extend(standings.into_iter().map(|(id, standing)| {
            let wait = match standing {
                Standing::Wait(wait) => wait,
                // About to start, or to be taken.
                Standing::Start => Wait::Next,
                Standing::Join(_) => Wait::After,
            };
            (id, wait)
        }));
    }
    let now = now_ms();
    let (mut live, mut ended): (Vec<Item>, Vec<Item>) = items.into_iter().partition(|i| !i.status.ended());
    live.sort_by_key(|i| {
        (i.status == Status::Waiting, if i.status == Status::Waiting { i.order } else { 0 }, i.started_at)
    });
    ended.retain(|i| i.ended_at.is_some_and(|at| now - at < RECENT_MS));
    ended.sort_by_key(|i| std::cmp::Reverse(i.ended_at));
    ended.truncate(RECENT_MOST);
    live.into_iter()
        .chain(ended)
        .map(|item| {
            let rt = item.task.as_deref().filter(|_| item.status == Status::Working).map(|task| hub.runtime(task));
            let status = if asks_user(hub, &item) { Status::NeedsYou } else { item.status };
            let wait = waits.remove(&item.id);
            Shown { status, activity: rt.map(|rt| rt.activity).unwrap_or_default(), wait, item }
        })
        .collect()
}

/// What the bot sees of its box: [`shown`] without the tasks the user started, which are theirs.
pub(super) fn seen_by_bot(hub: &Hub, cfg: &BotConfig) -> Vec<Shown> {
    shown(hub, cfg).into_iter().filter(|s| !s.item.source.is_user()).collect()
}

/// Where `item`'s task works, of its row's folder `cwd`: not a worktree that had nothing in it and went.
fn folder_of<'a>(item: &Item, cwd: Option<&'a String>) -> Option<&'a String> {
    let gone = |worktree: &Worktree| worktree.left == Left::Removed && cwd == Some(&worktree.cwd);
    cwd.filter(|_| !item.worktree.as_ref().is_some_and(gone))
}

/// The bot's `work` on the wire.
pub fn wire(hub: &Hub, cfg: &BotConfig) -> Value {
    let shown = shown(hub, cfg);
    // The one seat a task waiting on the user lends: past it, one more waiting gives nothing (`dispatch::free_seats`).
    let mut lent = false;
    // Each task: where it works (a worktree's folder, or the bot's as it was when the task began) and on what agent.
    let tasks: HashMap<String, BotConfig> = if shown.is_empty() {
        HashMap::new()
    } else {
        super::of(&hub.store, &cfg.id).into_iter().map(|task| (task.id.clone(), task)).collect()
    };
    Value::Array(
        shown
            .into_iter()
            .map(|Shown { item, status, activity, wait }| {
                // It takes one of the bot's seats.
                let seat = item.source.seated()
                    && match status {
                        Status::Working => true,
                        Status::NeedsYou => std::mem::replace(&mut lent, true),
                        _ => false,
                    };
                let task = item.task.as_ref().and_then(|task| tasks.get(task));
                // An item waiting for its task already knows the agent that task will start on.
                let runs_on = task.map_or_else(
                    || item.setup.over(&crate::agent::fallback::tasks(cfg)).backend,
                    |t| agent_of(&hub.store, t),
                );
                row(item, status, &activity, wait, seat, task.map(|t| &t.cwd), Some(runs_on))
            })
            .collect(),
    )
}

/// The agent `task` runs on now, else the one it ran on last (a backend id): the fallback it is on or ended on, else
/// its own.
pub(super) fn agent_of(store: &Store, task: &BotConfig) -> String {
    crate::agent::fallback::effective(task, crate::agent::bot::kept_step(store, task)).backend
}

/// One of a bot's `work` rows: `item` as it reads now, `cwd` the folder of its task bot and `runs_on` the agent
/// its task runs on (or will), when known.
pub(super) fn row(
    item: Item,
    status: Status,
    activity: &str,
    wait: Option<Wait>,
    seat: bool,
    cwd: Option<&String>,
    runs_on: Option<String>,
) -> Value {
    let folder = folder_of(&item, cwd);
    let writable = item.source.own() && cwd.is_some();
    let mut row = json!({
        "id": item.id, "title": item.title, "status": status, "activity": activity, "task": item.task,
        "wait": wait, "cutIn": item.cut_in, "createdAt": item.created_at, "startedAt": item.started_at,
        "endedAt": item.ended_at,
        "result": item.result.map(|r| codync_agent::acp::truncate(&r, WIRE_RESULT)),
        // The user may write to its task: the bot's own work, whose task is still there (parked too); with files
        // too, which a host from before them doesn't say.
        "talk": writable,
        "talkFiles": writable,
        "seat": seat,
    });
    if let Some(folder) = folder {
        row["folder"] = folder.clone().into();
    }
    if let Some(runs_on) = runs_on {
        row["runsOn"] = runs_on.into();
    }
    if let Some(source) = item.source.wire() {
        row["source"] = source;
    }
    if let Some(worktree) = item.worktree.as_ref().and_then(Worktree::wire) {
        row["worktree"] = worktree;
    }
    // The agent a thread the user started runs on, where it isn't its bot's (always without a bot).
    if let Some(agent) = &item.setup.backend {
        row["agent"] = agent.clone().into();
    }
    row
}

/// The box in a few lines: counts and the ceiling, then the live and waiting titles with their ids. None when
/// nothing is live or waiting. `whose`: `Your` for the bot's own turns, `Its` for who asks about the bot (a call).
pub fn lines(hub: &Hub, cfg: &BotConfig, whose: &str) -> Option<String> {
    let live: Vec<Shown> = seen_by_bot(hub, cfg).into_iter().filter(|s| !s.status.ended()).collect();
    let live: Vec<_> = live.iter().map(|s| (s.status, s.item.title.as_str(), s.item.id.as_str())).collect();
    summary(&live, cfg.max_agents, whose)
}

/// What one of a bot's items does now, for who follows the bot from outside its chat (a call's voice).
pub struct Doing {
    pub item: String,
    pub title: String,
    /// Its task's activity line; empty when it says none.
    pub activity: String,
}

/// What each of `cfg`'s items at work does now, oldest first. One that waits on the user isn't at work.
pub fn doing(hub: &Hub, cfg: &BotConfig) -> Vec<Doing> {
    let mut items = load(&hub.store, &cfg.id);
    items.sort_by_key(|i| i.started_at);
    let at_work = items.into_iter().filter(|i| i.status == Status::Working).filter_map(|item| {
        let rt = hub.runtime(item.task.as_deref()?);
        (rt.status == BotStatus::Working).then_some(Doing { item: item.id, title: item.title, activity: rt.activity })
    });
    at_work.collect()
}

/// Whether `item` works and its task waits on the user (a card, a sign-in): it needs them, and lends its seat
/// (`dispatch::free_seats`).
pub(super) fn asks_user(hub: &Hub, item: &Item) -> bool {
    item.status == Status::Working
        && item.task.as_deref().is_some_and(|task| hub.runtime(task).status == BotStatus::NeedsInput)
}

/// How `cfg` stands to what shows it outside its chat (a Live Activity, an ongoing notification), given its
/// chat's own runtime: it needs the user while one of its items does, and works while its box holds anything
/// that isn't over: an item working or waiting, or one whose report its chat hasn't taken yet. Otherwise the
/// chat's own status. With it, when that work began.
pub fn outside(hub: &Hub, cfg: &BotConfig, chat: &crate::hub::Runtime) -> (BotStatus, Option<i64>) {
    let items = load(&hub.store, &cfg.id);
    let open: Vec<&Item> =
        items.iter().filter(|i| !i.status.ended() || (i.source.is_bot() && i.report.is_some())).collect();
    let status = if chat.status == BotStatus::NeedsInput || open.iter().any(|i| asks_user(hub, i)) {
        BotStatus::NeedsInput
    } else if open.is_empty() {
        chat.status
    } else {
        BotStatus::Working
    };
    let began = open.iter().map(|i| i.started_at.unwrap_or(i.created_at)).min();
    (status, chat.started_at.or(began))
}

/// The task bots that work on an item now, on the whole computer.
pub fn working_tasks(store: &Store) -> Vec<BotConfig> {
    let tasks = store.task_bots().unwrap_or_default().into_iter().map(|row| row.config);
    tasks
        .filter(|task| task.task_of.as_deref().is_some_and(|bot| of_task(&load(store, bot), &task.id).is_some()))
        .collect()
}

/// The setups the user's waiting threads will run with (`Setup::over` their bot's) where they name a login, each named
/// by its bot (a thread without one by its title): a login profile they run on must still be there when they start.
pub fn waiting_threads(store: &Store) -> Vec<BotConfig> {
    let bots = store.bots().unwrap_or_default().into_iter().filter(|row| !row.deleted);
    bots.flat_map(|row| {
        let cfg = row.config;
        let waiting = load(store, &cfg.id).into_iter();
        let named = waiting.filter(|i| i.status == Status::Waiting && i.setup.login_profile.is_some());
        let name = |i: &Item| if cfg.plain { i.title.clone() } else { cfg.name.clone() };
        named
            .map(|i| BotConfig { name: name(&i), ..i.setup.over(&crate::agent::fallback::tasks(&cfg)) })
            .collect::<Vec<_>>()
    })
    .collect()
}

/// Puts an item that `task` works on in `bot`'s box.
#[cfg(test)]
pub fn put_working_for_test(store: &Store, bot: &str, task: &str) {
    let item = json!({"id": "i1", "title": "Work", "brief": "x", "status": "working", "order": 1, "createdAt": 0,
        "task": task});
    store.kv_set(&key(bot), &json!([item]).to_string()).expect("saved");
}

fn summary(live: &[(Status, &str, &str)], most: u8, whose: &str) -> Option<String> {
    if live.is_empty() {
        return None;
    }
    let count = |wanted: fn(Status) -> bool| live.iter().filter(|(status, ..)| wanted(*status)).count();
    let mut text = format!(
        "{whose} work in the background: {} working, {} waiting; at most {most} run at once, the rest wait their turn.",
        count(|s| s != Status::Waiting),
        count(|s| s == Status::Waiting)
    );
    for (status, title, id) in live {
        let word = match status {
            Status::Waiting => "waiting",
            // An approval, a choice or a sign-in: its card in the chat says which.
            Status::NeedsYou => "needs the user (its card in the chat)",
            _ => "working",
        };
        let _ = write!(text, "\n- {word}: \"{title}\" ({id})");
    }
    Some(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_item_works_in_its_folder_else_the_default_project_else_where_the_bot_lives() {
        let bot: BotConfig = serde_json::from_value(
            json!({"id": "b", "name": "B", "backend": "claude", "cwd": "/home", "projects": ["/p1", "/p2"]}),
        )
        .expect("a bot");
        let item = |source: Source, cwd: Option<&str>| Item {
            source,
            setup: Setup { cwd: cwd.map(str::to_owned), ..Setup::default() },
            ..Item::waiting("t", "b", None, None, 0)
        };
        assert_eq!(item(Source::Bot, None).folder(&bot), "/p1");
        assert_eq!(item(Source::User, None).folder(&bot), "/p1");
        assert_eq!(item(Source::Bot, Some("/p2")).folder(&bot), "/p2");
        assert_eq!(
            item(Source::Request { id: None }, None).folder(&bot),
            "/home",
            "a request works where the bot lives"
        );
        let alone = BotConfig { projects: Vec::new(), ..bot.clone() };
        assert_eq!(item(Source::Bot, None).folder(&alone), "/home");

        let kind = |folder| FolderKind::of(&bot, folder);
        assert_eq!(
            (kind("/p2"), kind("/home"), kind("/tmp")),
            (FolderKind::Project, FolderKind::Home, FolderKind::Other)
        );
    }

    #[test]
    fn a_threads_setup_changes_only_what_it_names() {
        let bot: BotConfig = serde_json::from_value(json!({"id": "b", "name": "B", "backend": "claude", "cwd": "/bot",
            "model": "m1", "modelOptions": {"effort": "low"}, "permission": "auto", "createdAt": 0}))
        .expect("a bot");
        let setup: Setup = serde_json::from_value(json!({"model": {"id": null}, "approval": {"permission": "ask",
            "mode": "plan"}}))
        .expect("a setup");
        let run = setup.over(&bot);
        assert_eq!((run.cwd.as_str(), run.model, run.model_options.len()), ("/bot", None, 0));
        assert_eq!((run.permission, run.mode.as_deref()), (Permission::Ask, Some("plan")));
        assert!(Setup::default().is_bots() && !setup.is_bots());
        let on =
            |login: &str| Setup { login_profile: Some(login.to_owned()), ..Setup::default() }.over(&bot).login_profile;
        assert_eq!((on("work"), on("")), (Some("work".to_owned()), None), "a login profile, or the default login");
        // The login it picks leaves the bot's chain; another agent leaves the bot's Fast mode too.
        let chained = BotConfig {
            fast_mode: true,
            fallbacks: vec![
                serde_json::from_value(json!({"backend": "claude", "loginProfile": "work"})).expect("a step"),
            ],
            ..bot.clone()
        };
        let work = Setup { login_profile: Some("work".into()), ..Setup::default() }.over(&chained);
        assert!(work.fallbacks.is_empty() && work.fast_mode, "{work:?}");
        let codex = Setup { backend: Some("codex".into()), ..Setup::default() }.over(&chained);
        assert!(!codex.fast_mode && codex.fallbacks.is_empty());
        assert_eq!(serde_json::to_value(Setup::default().over(&bot)).ok(), serde_json::to_value(&bot).ok());
    }

    #[test]
    fn a_waiting_thread_on_a_login_is_named_with_it() {
        let hub = crate::chat::calls::context::tests::hub();
        let cfg = crate::chat::calls::context::tests::bot("b1", "Mika", "/tmp");
        hub.store.save_bot(&cfg).expect("saved");
        let on = |login: Option<&str>, status| Item {
            setup: Setup { login_profile: login.map(str::to_owned), ..Setup::default() },
            status,
            ..Item::waiting("Fix it", "x", None, None, 0)
        };
        let items = [on(Some("work"), Status::Waiting), on(None, Status::Waiting), on(Some("old"), Status::Done)];
        hub.store.kv_set(&key("b1"), &serde_json::to_string(&items).expect("json")).expect("saved");
        let waiting: Vec<_> = waiting_threads(&hub.store).into_iter().map(|c| (c.name, c.login_profile)).collect();
        assert_eq!(waiting, [("Mika".to_owned(), Some("work".to_owned()))]);
    }

    #[test]
    fn the_bot_reads_its_box_in_a_few_lines() {
        assert_eq!(summary(&[], 3, "Your"), None, "an empty box says nothing");
        let live = [
            (Status::Working, "Fix login", "i1"),
            (Status::NeedsYou, "Run the suite", "i2"),
            (Status::Waiting, "Docs", "i3"),
        ];
        let text = summary(&live, 2, "Your");
        assert_eq!(
            text.as_deref(),
            Some(
                "Your work in the background: 2 working, 1 waiting; at most 2 run at once, the rest wait their \
                 turn.\n- working: \"Fix login\" (i1)\n\
                 - needs the user (its card in the chat): \"Run the suite\" (i2)\n- waiting: \"Docs\" (i3)"
            )
        );
        let about = summary(&live[..1], 1, "Its").expect("lines");
        assert!(
            about.starts_with("Its work in the background: 1 working, 0 waiting; at most 1 run at once"),
            "{about}"
        );
    }

    #[test]
    fn a_worktree_that_was_removed_is_no_folder_to_show() {
        let mut item: Item = serde_json::from_value(json!({
            "id": "i", "title": "t", "brief": "b", "status": "done", "order": 1, "createdAt": 0, "task": "task-1",
            "worktree": {"branch": "codync/t", "path": "/w", "cwd": "/w/app", "base": "abc", "left": "kept"},
        }))
        .expect("an item");
        let (worktree, workspace) = ("/w/app".to_owned(), "/repo/app".to_owned());
        assert_eq!(folder_of(&item, Some(&worktree)), Some(&worktree), "kept: it is there to go to");
        item.worktree.as_mut().expect("a worktree").left = Left::Removed;
        assert_eq!(folder_of(&item, Some(&worktree)), None, "it went with its branch");
        assert_eq!(folder_of(&item, Some(&workspace)), Some(&workspace), "reopened: its task is in the bot's folder");
        assert_eq!(folder_of(&item, None), None);
    }

    #[test]
    fn an_item_says_where_it_came_from_unless_the_bot_added_it() {
        assert_eq!(Source::Bot.wire(), None, "absent: an older client shows the row as the bot's own");
        let routine = Source::Routine { id: "r1".into(), run: "run1".into() };
        assert_eq!(routine.wire(), Some(json!({"kind": "routine", "id": "r1"})));
        assert_eq!(Source::Request { id: Some("b2".into()) }.wire(), Some(json!({"kind": "request", "id": "b2"})));
        assert_eq!(Source::Request { id: None }.wire(), Some(json!({"kind": "request"})), "asked from elsewhere");
        // An item saved before sources is the bot's own.
        let old: Item = serde_json::from_value(json!({
            "id": "i", "title": "t", "brief": "b", "status": "waiting", "order": 1, "createdAt": 0,
        }))
        .expect("an item");
        assert!(old.source.is_bot());
        assert!(json!(old).get("source").is_none());
    }

    #[test]
    fn outside_its_chat_a_bot_works_while_its_box_does_and_needs_the_user_when_an_item_does() {
        let hub = crate::chat::calls::context::tests::hub();
        let cfg = crate::chat::calls::context::tests::bot("b-out", "Mika", "/tmp");
        hub.store.save_bot(&cfg).expect("saved");
        let stands = || hub.outside_runtime(&cfg).status;
        assert_eq!(stands(), BotStatus::Idle);
        hub.set_runtime(&cfg.id, |r| r.status = BotStatus::Error);
        assert_eq!(stands(), BotStatus::Error, "the chat's own, with an empty box");

        let item = |status: &str, more: Value| {
            let mut item = json!({"id": "i1", "title": "Work", "brief": "x", "status": status, "order": 1,
                "createdAt": 5, "task": "task-out"});
            item.as_object_mut().expect("an object").extend(more.as_object().cloned().unwrap_or_default());
            hub.store.kv_set(&key(&cfg.id), &json!([item]).to_string()).expect("saved");
        };
        item("waiting", json!({}));
        assert_eq!(stands(), BotStatus::Working, "what waits will start");
        item("working", json!({"startedAt": 9}));
        let rt = hub.outside_runtime(&cfg);
        assert_eq!((rt.status, rt.started_at), (BotStatus::Working, Some(9)), "since its item began");
        hub.set_runtime("task-out", |r| r.status = BotStatus::NeedsInput);
        assert_eq!(stands(), BotStatus::NeedsInput, "its task's card waits");
        // The same on the wire, beside the chat's own status.
        let row = hub.store.bot(&cfg.id).expect("read").expect("a bot");
        let wire = hub.bot_json(&row);
        assert_eq!(
            (&wire["status"], &wire["outside"]),
            (&json!("error"), &json!({"status": "needsInput", "startedAt": 9}))
        );
        // Done, but the chat hasn't taken its report yet: the bot still has that to say.
        let report = json!({"item": "i1", "title": "Work", "outcome": {"Ok": "done"}, "hops": 0});
        item("done", json!({"endedAt": 10, "report": report}));
        assert_eq!(stands(), BotStatus::Working);
        item("done", json!({"endedAt": 10}));
        assert_eq!(stands(), BotStatus::Error, "the box is empty: the chat's own again");
        item("stopped", json!({"endedAt": 10}));
        hub.set_runtime(&cfg.id, |r| r.status = BotStatus::Idle);
        assert_eq!(stands(), BotStatus::Idle);
    }

    #[test]
    fn a_row_names_the_agent_its_task_runs_on() {
        let hub = crate::chat::calls::context::tests::hub();
        let mut cfg = crate::chat::calls::context::tests::bot("b-agent", "Mika", "/tmp");
        let mut task = crate::chat::calls::context::tests::bot("task-a", "Mika", "/tmp");
        task.task_of = Some(cfg.id.clone());
        task.backend = "claude".into();
        task.fallbacks = vec![crate::store::Fallback { backend: "codex".into(), ..Default::default() }];
        hub.store.save_bot(&cfg).expect("saved");
        hub.store.save_bot(&task).expect("saved");
        let items = json!([{"id": "a", "title": "a", "brief": "x", "status": "working", "order": 1, "createdAt": 1,
            "startedAt": 1, "task": "task-a"}]);
        hub.store.kv_set(&key(&cfg.id), &items.to_string()).expect("saved");
        let runs_on = || wire(&hub, &cfg)[0]["runsOn"].clone();
        assert_eq!(runs_on(), "claude", "its own agent");
        // On its fallback, as the actor keeps it: still so once the task's actor and its runtime are gone.
        hub.store.kv_set("fallback.on.task-a", &crate::agent::fallback::login(&task, 1)).expect("saved");
        assert_eq!(runs_on(), "codex", "the fallback it switched to");
        // Waiting for a seat, without a task yet: the agent its task will start on, in Advisor mode the tasks'.
        cfg.task_agent = Some(crate::store::Fallback { backend: "gemini".into(), ..Default::default() });
        let items = json!([{"id": "w", "title": "w", "brief": "x", "status": "waiting", "order": 1, "createdAt": 1}]);
        hub.store.kv_set(&key(&cfg.id), &items.to_string()).expect("saved");
        assert_eq!(wire(&hub, &cfg)[0]["runsOn"], "gemini");
    }

    #[test]
    fn only_one_task_waiting_on_the_user_lends_its_seat() {
        let hub = crate::chat::calls::context::tests::hub();
        let cfg = crate::chat::calls::context::tests::bot("b-seat", "Mika", "/tmp");
        hub.store.save_bot(&cfg).expect("saved");
        let item = |id: &str, at: i64| {
            json!({"id": id, "title": id, "brief": "x", "status": "working", "order": 1, "createdAt": at,
                "startedAt": at, "task": format!("task-{id}")})
        };
        let items = json!([item("a", 1), item("b", 2), item("c", 3)]);
        hub.store.kv_set(&key(&cfg.id), &items.to_string()).expect("saved");
        let seats =
            || -> Vec<Value> { wire(&hub, &cfg).as_array().expect("rows").iter().map(|r| r["seat"].clone()).collect() };
        assert_eq!(seats(), [true, true, true]);
        hub.set_runtime("task-a", |r| r.status = BotStatus::NeedsInput);
        hub.set_runtime("task-b", |r| r.status = BotStatus::NeedsInput);
        assert_eq!(seats(), [false, true, true], "one seat is lent, as the dispatcher counts it");

        let mut items = items;
        items[2]["source"] = json!({"kind": "user"});
        hub.store.kv_set(&key(&cfg.id), &items.to_string()).expect("saved");
        hub.set_runtime("task-c", |r| r.status = BotStatus::NeedsInput);
        hub.set_runtime("task-a", |r| r.status = BotStatus::Working);
        assert_eq!(seats(), [true, false, false], "the user's task takes no seat and leaves the lent one to b");
    }

    #[test]
    fn the_six_words_are_what_the_wire_says() {
        let words: Vec<Value> =
            [Status::Waiting, Status::Working, Status::NeedsYou, Status::Done, Status::Failed, Status::Stopped]
                .into_iter()
                .map(|s| json!(s))
                .collect();
        assert_eq!(words, ["waiting", "working", "needsYou", "done", "failed", "stopped"]);
    }
}
