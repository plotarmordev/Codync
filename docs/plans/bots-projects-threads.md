# Bots, projects and tasks

Status (2026-11-25): every part below is built on `dev`; only [Open](#open) is left for the owner. One plan for where a bot lives, where its work happens and what a task is given. It replaces
the separate "bot home and projects" and "task profile" plans. Answers part of stage A in
[personal agent: daily and project work](personal-agent-experience.md). What is built, decided or only proposed:

| Part | State |
| --- | --- |
| Recap when a bot's folder changes | **Built**, in dev (`a5fccdbf7`) |
| [Bot home and projects](#3-bot-home-and-projects) | **Built** (stages 5 and 6), host and all four clients |
| [Worktree mode](#worktree-mode) (Off / Side by side / Always) | **Built** (stage 5), host and all four clients |
| [Worktrees counted per repository](#count-side-by-side-per-repository) | **Built** (stage 5) |
| [Task profile, no fork](#7-task-profile-no-fork) | **Built** (stage 2) |
| [A user's task leaves the bot's queue](#6-a-users-task-leaves-the-bots-queue) | **Built** (stage 3) |
| [An item names its place](#an-item-names-its-place) (`add_work` `place`) | **Built** (stage 7) |
| [Filling a new worktree](#filling-a-new-worktree) (`.worktreeinclude`, shared folders) | **Built** (stage 7) |
| [Tasks without a bot get a folder of their own](#5-tasks-without-a-bot-get-a-folder-of-their-own) | **Built** (stage 7) |
| [`ask_work`](#8-the-bot-watching-its-tasks): the bot asks a task and gets an answer | **Built** (stage 7) |

## Words

As in the [Names](../architecture/file-structure.md#names) table; this plan uses no others.

| Said in discussion | The word | In code |
| --- | --- | --- |
| coordinator, main bot | **bot**; where the user talks to it: **chat** | `BotConfig` |
| worker, task-worker | **task** | a task bot: `BotConfig::task_of`, id `task-<uuid>` |
| one piece of work | **item** | `Item` |
| what a task is told | **brief** | `brief` |
| what it hands back | **report** | `report` |
| the queue | **event box** | kv `work.<bot>` |
| what an agent remembers now | **session** | a task *has* sessions, never *is* one |
| project folder | **working folder**: a **project** (the user's) or an **own folder** (Codync's) | `cwd` |

"Coordinator" is on the Names table's Never list (for chat); "worker" is no product word.

## 1. The model

```
bot ─▶ project ─▶ task ─▶ sessions
```

- **Bot**: hands work out and keeps its chat free. It lives in its own folder; it can send work to several projects,
  one of them its default.
- **Project**: where code work lives. A task's work is always in a project (or the task's own folder); a git
  worktree of a project is Codync's to manage, kept in Codync's data folder (`~/.codync/worktrees/<bot>/<item>`,
  `~/.codync-dev/…` for Codync Dev; `chat/tasks/worktree.rs` `root()`), never inside the project.
- **Task**: does one item at a time, may go on to the next (`after`), and is told only what its item needs.
- **Sessions**: a fallback or a restart gives a task a new session; the task goes on.

Three ways work starts:

| | The bot hands it out (`add_work`) | The user starts a task with a bot | The user starts a task without a bot |
| --- | --- | --- | --- |
| Where it waits | The bot's event box, under `maxAgents` | No wait: starts at once ([§6](#6-a-users-task-leaves-the-bots-queue)) | No wait: starts at once, no ceiling on the computer |
| What the task is told | The bot's brief ([§7](#7-task-profile-no-fork)) | The bot's profile and memory, then the user's own words ([§7](#a-users-task-is-the-bot)) | The user's own words, as written, nothing of Codync's |
| What it has of the bot | Skills, memory (to look up), tools for its source | Name, description, skills, memory opening, the same tools | Nothing: the agent's own tools |
| Its result | A report the bot turns into a message in the chat | Read in the task | Read in the task |
| The bot sees and steers it | Yes | No | There is no bot |

## 2. Before this plan (dev, 2026-11-24)

What the code did, read from it, before any change here. Kept as the reason for each part; what runs now is §1 and
[work](../features/work.md).

### Where a task works

| Who started it | Its folder |
| --- | --- |
| The bot (`add_work`) | The bot's `cwd`: its own folder (`~/.codync/bots/<id>/workspace`) or its project. `add_work` takes no folder |
| The user, with a bot | `setup.cwd` if picked, else the bot's `cwd` |
| The user, without a bot | The folder picked, else the user's **home folder** (`ThreadStart.tsx` sends `cwd: home`; the box `tasks::plain` has `cwd` = home) |
| An item that follows another (`after`) | That task's folder or worktree |
| A routine's run, another bot's request | The bot's `cwd`, never a worktree |

It gets a worktree only when all hold (`dispatch::plan`, `beside`): the folder is a git repository, the bot's
`taskWorktrees` is on, the folder isn't the bot's own, and **another task of the bot is already working**
(`side_by_side` = any working task of the bot, in any folder). So the first task works in the user's checkout.

### What a task is given

- **The bot's whole system prompt** (`agent/bot/session.rs` renders `context::render(&hub.store, &self.cfg.as_owner())`;
  only the Workspace line is the task's): name and description, the Environment paragraph, Messages, Other
  conversations, Other computers, Workspace, `dev_servers::BRIEF`, `routines::BRIEF`, **`tasks::BRIEF`** (call
  `add_work`), skills, `KEEP`, the bot's memory opening.
- **A contradiction**: `tasks::BRIEF` tells it to hand work on with `add_work`, which is refused in a task; the item
  prompt (`tasks::prompts::item_prompt`) overrides it each time.
- **The bot's whole tool set**: `connectors`, `chat`, `team`, `memory`, `routines`, `workflows`, Composio
  (`Session::mcp_servers`); only `computer` and `manage` are off (`dispatch::task_bot`). What a task may not
  do is refused **when called** (`tasks::refuse_in_task`, `refuse_chat_only`, `acting_as`), so it reads the box,
  profile, fallback, `team` and `routines` tool descriptions for nothing.
- **A fork of the chat's session** for the bot's own item in its folder on its own agent and login
  (`item.fork_from`, `dispatch::fork_source`, `sources::from_chat`): the task carries the whole chat.

### How a user's task is scheduled

`startThread` with a bot (`sources::add_thread`, `Source::User`) puts an item in the bot's box: it waits for a place
under `maxAgents` beside the bot's own items and for the bot's own turn to end (`dispatch::standings`,
`capacity.chat_free`), and the bot reads it in the box summary of every turn (`items::lines` → `shown`, no filter
by source) and in `work_status`. Without a bot it goes to the hidden box `Agents` (`BotConfig::plain`), never
waits on a bot, and shares that box's 6 places.

### Where it can go wrong

From the code above, most harm first:

1. **The first task works in the user's checkout.** No worktree until a second task runs; the user editing the
   same files at the same time collides with it. ([Worktree mode](#worktree-mode) **Always** fixes it.)
2. **A new worktree is empty of what isn't committed**: no `node_modules`, no `.env`, none of the user's
   uncommitted changes. A task reinstalls or fails on missing config. ([Filling a new worktree](#filling-a-new-worktree).)
3. **Handing work out is a prompt rule only** (`tasks::BRIEF`, `EACH_TURN`): a bot that does long work in its chat
   leaves the user without it until the turn ends.
4. **A user's task queues behind the bot's work** and is shown to the bot. ([§6](#6-a-users-task-leaves-the-bots-queue).)
5. **A bot reaches one folder**: `add_work` has no folder. ([§3](#3-bot-home-and-projects).)
6. **A task carries the chat**: by fork, every word of it; without one, the whole chat prompt and tool list.
   ([§7](#7-task-profile-no-fork).)
7. **Kept worktrees pile up**: they stay until merged and are looked at again only at host start.
8. **Side by side is counted across repositories**: a task in B gets a worktree because one works in A (the
   `ponytail` note in `dispatch.rs`).
9. **Tasks without a bot share the home folder**: up to 6 at once in `~`, no versions. ([§5](#5-tasks-without-a-bot-get-a-folder-of-their-own).)

## 3. Bot home and projects

### Problem

A bot has one folder, `cwd`: its **own folder** (`~/.codync/bots/<id>/workspace`, the default) or a **project** the
user picks in its settings. That one field means where the bot *is* (its chat, routines, calls, group turns) and
where *code work* happens.

- **Moving the desk reset the chat's context.** Any `cwd` change restarts the agent (`agent/bot/queue.rs`, the
  `restart` check) with a fresh session. Fixed: the recap now carries over (`a5fccdbf7`).
- **Daily work lands in the repository.** With a project as `cwd`, the chat, every routine run and every group turn
  start in the user's checkout; files the bot writes for itself go there, with no versions.
- **The bot can't send work elsewhere.** `add_work` has no folder, so a bot that should work on two repositories
  needs two bots.

What a project as `cwd` gives today, and keeps giving to tasks after the change: the agent loads the project's
`CLAUDE.md` / `AGENTS.md`, `.mcp.json`, project skills and the agent's own memory for that folder.

### Proposal

| | Today | Proposed |
| --- | --- | --- |
| Bot's `cwd` | Own folder or a project | **Always its own folder** (its home) |
| Projects | One, as the bot's `cwd` | A list on the bot, `projects` (0..n), the first its default |
| Bot's own tasks (`add_work`) | The bot's `cwd` | A folder the bot names (`folder`), else its default project, else its home |
| Tasks the user starts with the bot | Bot's `cwd` or a picked folder | The bot's default project, else its home; any folder can still be picked |
| Routine runs, group turns, calls | Bot's `cwd` | Home; a routine may name a folder |

```
~/.codync/
  bots/<bot id>/
    workspace/          the bot's cwd, always (git versions per turn, uploads/ inside)
    bot-memory/         this bot's memory
    artifacts/, files/  what it published / file snapshots
  shared-memory/        "yours", shared by every bot that uses it
  worktrees/<bot id>/<item id>/   worktrees Codync makes in a project
  tasks/<item id>/    a task without a bot (§5, proposed)

<a project the user owns>/   never written by Codync itself; tasks work here or in a worktree of it
```

Nothing of the bot's (identity, memory, uploads) goes into a project.

### Host

- `BotConfig`: `projects: Vec<String>` (absolute, existing folders, validated like `cwd` in `hub/validate.rs`;
  registry JSON stays untrusted). `cwd` is always `agent::workspace::path(id)`.
- `add_work` gains `folder` (any absolute path; decision 3) and `place` ([below](#an-item-names-its-place)). The bot's
  instructions (`chat/context.rs`, the Workspace line) list its projects and say: code work in a project goes to a
  item with that folder; read a project's files with explicit paths; never change its own folder.
- `startThread` without `setup.cwd` uses the bot's first project, else its home.
- Routines: optional `folder` (a project, or home when absent).
- Everything that creates or edits a bot with a project `cwd` maps it into `projects`: `createBot`/`updateBot`,
  the manage tools (`chat/manage.rs`), import (`import/mod.rs`), templates and the marketplace check
  (`market/verify.rs`).
- What reads `cfg.cwd` and means "the project" moves to the task's or the default project: the terminal tab
  (`api/bots.rs` `hub.terms.shell`), a call's "Folder" line (`chat/calls/context.rs`), the team directory
  (`chat/team/directory.rs`), `start_dev_server`'s base (`dev_servers/tools.rs`), `gitRepo` on the wire
  (`hub/sync.rs`: per project).
- `session.rs` `avoid_clashes` reads MCP server names from the folder a session starts in: at home it no longer sees
  a project's `.mcp.json`; a task in the project does.

### Migration

At host start, once: a bot whose `cwd` isn't its own folder gets `projects = [cwd]`, `cwd` = its own folder (made and
versioned as for a new bot), and a "recap owed" mark the actor reads at start (the recap needs a live session, which a
migration before the actor runs doesn't have). Groups and tasks are not touched; a working task keeps its folder
until its item ends. Moving the bot home flips `workspace::is_managed`, so the migration also:

- **Stamps `sharedMemory: false`** where unset (it is derived from `is_managed` when unset, `chat/memory/mod.rs`),
  so a project bot doesn't silently start reading and writing "yours".
- **Moves its uploads**: `bots/<id>/uploads` into `workspace/uploads` (`uploads::root` changes with `is_managed`).
- **Stamps each routine with the old folder**, so it keeps running where it did.
- Notes that the harness's own memory changes folder: `own_memory` writes its switch into the home folder, and the
  Memory screen's Claude Code section (`chat/memory/api.rs` `project_memory`, keyed by `cwd`) shows the home's.

### What the chat gives up

At home the chat's agent no longer loads the project's `CLAUDE.md` / `AGENTS.md`, `.mcp.json`, project skills or
the agent's memory for that folder; tasks in the project still do. Questions about the code are answered by
reading with explicit paths or by a task. That is the point (daily context stays clean), and a visible change for
users who chat with a "coding bot": the manual says it.

### Wire and compatibility

Additive ([compatibility](../reference/compatibility.md)): bots gain `projects`; `cwd` stays (always the home). An
older app that sets `cwd` to a project gets it moved into `projects` as the first one, so its "Choose project
folder…" keeps doing what the user meant; it shows **Own folder** afterwards and can't list projects (decision 5).

### Clients (all four, same change)

- Bot settings, **Workspace** → **Projects**: the list (add with the folder picker and its recent workspaces, remove,
  make default) and the [worktree mode](#worktree-mode) under it when a project is a git repository. **Own folder** is
  no longer a choice; the home path shows read-only with a copy action.
- New thread (desktop Threads page; phones and TUI once they start tasks): the folder chip lists the bot's projects
  first, then recent, then browse.
- Desktop, iPhone, Android, terminal UI; user manual (`web/content/docs/bots.mdx` "Workspace", `working-folders`,
  `conversations.mdx`), and the site's mockups of bot settings.

### Analytics

`bot_project_added` / `bot_project_removed` (count only), `work_added` gains `folder: home | project | other`,
`bot_projects_migrated` (count). Never paths.

## 4. Worktrees

### The rules, applied to the task's folder

- **Where**: `~/.codync/worktrees/<bot id>/<item id>`, branch `codync/<title-in-lowercase-dashes>` (`-2`, `-3` when
  taken), from the commit the project is on. When the task's folder is a subfolder of the repository, the agent
  starts in the same subfolder of the worktree.
- **When**: by the bot's [worktree mode](#worktree-mode), or the item's own [place](#an-item-names-its-place). Routine
  runs and other bots' requests never get one; an item that follows another stays in that one's folder; the bot's
  home and a task's own folder never get one.
- **After**: a worktree with nothing uncommitted whose branch is unchanged or merged is removed with its branch;
  anything else is kept and the report names the branch and folder. Orphans go once merged (a week after their last
  item, or when the bot is deleted). Codync never merges, rebases, resets or force-deletes a branch with work on it,
  and never touches a worktree it didn't make.
- **Fails**: no commit yet, git missing, a checkout hook fails: the task works in the folder itself, logged.

### Worktree mode

Decided (1). `taskWorktrees` (a bool) becomes `worktreeMode`: `off | sideBySide | always`, `sideBySide` unless set.
`beside` in `dispatch.rs` becomes: `always`, or `sideBySide` and another task works in the same repository.
**Always** gives every task of the bot's own work or the user's a worktree, the first included, so the user's
checkout is never touched. The wire keeps `taskWorktrees` for older apps (true unless `off`; turning it on from
`off` sets `sideBySide`, off sets `off`, so saving another field never drops `always`). Settings shows the three
choices with one line each. Analytics: `worktree_mode` on `bot_updated`, `place` on `work_added`.

### Count side by side per repository

Decided. `side_by_side` is `items::working(items) > 0` today. With tasks in several projects, count only working
tasks whose folder is in the same repository (same `git rev-parse --show-toplevel`). Removes the `ponytail` note.

### An item names its place

Proposed. `add_work` gains `place: here | worktree` (absent: the bot's mode decides). The bot knows whether an item
edits code the user may be touching, or should build on the work of the task before it; the mode is the default,
the item the exception. `startThread`'s setup takes the same choice from the new-task page.

### Filling a new worktree

Proposed. A new worktree is a clean checkout: no ignored folders (`node_modules`, `target`, `.venv`), no local
config (`.env`), none of the user's uncommitted changes. Two lists the project keeps, read from the project's main
checkout:

- **`.worktreeinclude`** at the repository root: ignored files to **copy** into each new worktree (`.env`,
  `.env.local`, editor settings). Literal paths only; a path that is tracked, missing or not ignored is skipped.
- **Shared folders**: ignored folders to **link** into each new worktree (`node_modules`, `.cache`), set per project
  in Codync (a project row's settings) so nothing Codync-specific has to be committed.

Copied or linked before the agent starts, so the first turn finds them. The user's uncommitted changes are never
carried: the report says the worktree started from the last commit. Each copy or link failure is logged and counted
(`worktree_fill_failed`, `stage`), never fatal.

### Cleanup

Kept worktrees were looked at again only at host start. With tasks in several
projects that can pile up, so the hourly look (`sweep.rs`) also checks them. Built with projects (stage 5).

### How another tool does it

Orca (an open-source agent IDE, read from its docs and `worktree-logic.ts`, 2026-11-24): every item gets a worktree,
never the main checkout; the one handing out work names its place each time (`--worktree current | new-child |
new-top-level`, `--repo` for another project, `--on` for another computer); worktrees live outside the
repository under one folder (`<workspaceDir>/<repo>/<name>`, per repository overridable); and `.worktreeinclude`,
shared folders and a setup script fill a new one before the agent starts. [An item names its place](#an-item-names-its-place)
and [filling a new worktree](#filling-a-new-worktree) take the two ideas that fix the first two problems in [§2](#where-it-can-go-wrong).

## 5. Tasks without a bot get a folder of their own

Proposed. Today such a task works in the user's home folder unless one is picked (§2): no separation between up to
6 at once, no versions, and a home folder kept in git (dotfiles) can be given a worktree of itself.

Each task without a bot gets `<data dir>/tasks/<item id>/`, made when its task starts and versioned like a bot's
own folder (`agent::versions`: `git init`, the default `.gitignore`, a save before and after each turn). It is the
default choice; **Browse other folders** still picks a project.

- Host: `sources::add_thread` fills `setup.cwd` with it when a task without a bot has none;
  `workspace::own_folder` counts it as the task's own, so it never gets a worktree. It goes with the task bot
  (seven days after the item ends, as its transcript does).
- Desktop: the folder choice's first item reads **Own folder** instead of the home folder's name; the app stops
  sending `cwd: home` only when `hello` says the host makes the folder (a new flag), so an older host keeps today's.
- Phones and TUI don't start tasks today: nothing to change.
- Analytics: `work_added`'s `folder` is `own | other` for tasks without a bot.
- Open: whether its files outlive the task.

## 6. A user's task leaves the bot's queue

Decided (owner, 2026-11-24; replaces the same day's "keep the shared scheduling"). A task the user starts is the
user's, not the bot's dispatch. It keeps its **item record** (stop, steer, try again, `send` to a task,
`recall_work` for the user's apps, the watchdog, the hourly look and the worktree rules are built on it) and leaves
the **queue** and the bot's **view**.

| | Today | Decided |
| --- | --- | --- |
| Where the item lives | The bot's box | Same box, same `Item`, `source: user` |
| Counts toward `maxAgents` | Yes | **No** |
| Waits for the bot's own turn | Yes (`chat_free`) | **No**: nothing of that turn is in it, and stopping that turn can't drop it |
| Near the usage limit | One task at a time | Unchanged |
| Counts for side by side (worktrees) | Yes | Yes: two tasks in one repository still need two folders |
| In the bot's box summary, `work_status`, `recall_work` | Yes | **No** |
| `steer_work` / `tell_work` / `ask_work` from the bot | Allowed | Refused: not the bot's |
| On the Threads page, steering, messages | As a thread | Unchanged |

- `dispatch::standings`: a waiting `Source::User` item starts whenever it isn't `held` and the usage guard allows,
  without taking from `free`. Tests: `a_users_task_starts_past_the_ceiling`.
- `items::shown` gets a view for the bot that leaves `Source::User` out (`lines`, `work_status`, `recall`); the
  clients' wire keeps every item.
- `steer.rs` / `thread.rs` `tell`: refuse `Source::User` when the bot's agent calls; the user's own `workFirst`,
  `stopWork`, `retryWork`, `send` stay.
- No ceiling is a choice: the user started each by hand. If it ever floods a computer, add a per-computer ceiling for
  user tasks, not a share of the bot's.
- Analytics: `waited_ms` on a task's start.

## 7. Task profile, no fork

Decided (owner, 2026-11-24): a task is told only what its item needs, and **no task forks the chat's session**:
the chat talks about many things that have nothing to do with the item, and the brief is the one place what the item
needs is written.

### What every task gets

`chat/context.rs` `render_task(store, cfg, item)` beside `render` (which stays the chat's):

| Part | What it says |
| --- | --- |
| The item | The brief (the bot's, a routine's prompt, another bot's request) or the user's own words |
| Where | Its folder or worktree and branch; not to change the bot's settings to work elsewhere |
| How it ends | Its last message is its report (to the bot's chat, or kept in the task); `request_decision` when it needs the user; `send_file` and artifacts to share |
| Skills | The bot's skills brief (`market::skills_brief`): how the work gets done |
| Memory | Not in the prompt: the `memory` server to look things up, and the brief carries what the bot knows matters. A user's task is the exception ([below](#a-users-task-is-the-bot)) |
| Environment | One line: it runs inside Codync, so it doesn't point the user at the agent's own CLI |
| Dev servers | `dev_servers::BRIEF`: a server it starts is the bot's, listed and stopped there |

No task gets: Messages, Other conversations, Other computers, `routines::BRIEF`, `tasks::BRIEF`, the chat so far.
The bot's name, description and memory opening go only to a user's task ([below](#a-users-task-is-the-bot)).

### A user's task is the bot

Decided (owner, 2026-11-24): a task the user starts with a bot is the user talking to **that bot**, in a new
conversation in the task's folder. The bot handed nothing out and wrote no brief, so the task can't be told
"only the item": it is told who it is, as the bot is.

| | Bot's item (brief) | User's task |
| --- | --- | --- |
| Who it is | A task doing one item; no name | **The bot**: its name and description (the bot's own instructions) |
| Memory | Looked up with the `memory` server | **The bot's memory opening** (its own layer and "yours", as the chat gets it), and the `memory` server |
| Skills, Environment, Workspace, dev servers | ✔ | ✔ |
| The project's `CLAUDE.md` / `AGENTS.md`, `.mcp.json`, project skills | Loaded by the agent from its folder | Same |
| The chat so far | ✗ | ✗: a new conversation |
| Messages, Other conversations, Other computers, `routines::BRIEF` | ✗ | ✗: it has no chat, room or request to answer |
| `tasks::BRIEF`, box tools (`add_work`, …) | ✗ | ✗: a task runs no tasks |
| How it ends | Report to the bot's chat | The user reads its answers here and may write to it |

So `render_task` for `Source::User` is the chat's profile part (`render`'s name, description, Environment,
Workspace, dev servers, skills, `KEEP`, memory opening) without its chat-only paragraphs, plus the "How it ends" line
for a task the user talks to. The setup the user picked for it (model, approvals, login, agent) applies to this
task only; the bot's own settings don't change. The bot itself still doesn't see the task (§6).

### By source

| | Bot's item | User's task | Routine run | Bot request |
| --- | --- | --- | --- | --- |
| `chat`: `request_decision`, `send_file`, artifacts | ✔ | ✔ | as today | as today |
| `chat`: box, profile, fallback tools | ✗ | ✗ | ✗ | ✗ |
| `memory`, connectors, Composio, `workflows` | ✔ | ✔ | ✔ | ✔ |
| `routines` + its paragraph | ✗ | ✗ | ✔ (progress note, follow-ups) | ✔ (an asked bot runs its own routine) |
| `team` (`list_bots`, `message_bot`) | ✗ | ✗ | ✔ | ✔ (`message_bot` refused with why, as in the chat) |
| `computer`, `manage` | ✗ | ✗ | ✗ | ✗ |

The routine and request columns are today's rules (`background-tasks.md`, "Other sources") written as a profile.

### The tools, by name

Besides the agent's own tools (files, shell, web) and what the agent loads from its folder itself (the project's
`CLAUDE.md` / `AGENTS.md`, `.mcp.json`, project skills):

| Server | A task lists | A task doesn't list |
| --- | --- | --- |
| `chat` | `request_decision`, `send_file`, `publish_artifact`, `list_artifacts`, `start_dev_server`, `list_dev_servers`, `dev_server_logs`, `stop_dev_server` | `send_message` (its report is its message), the reaction tool (no chat message to react to), the save-a-skill tool (open: a task that worked out a way of working says so in its report), `add_work`, `steer_work`, `tell_work`, `work_status`, `recall_work`, the profile and fallback tools |
| `memory` | `search_memory`, `read_memory`, `search_history`, `edit_memory` (writes the bot's memory as the task's own turn, as today) | |
| connectors, Composio, `workflows` | as the bot has them | |
| `routines`, `team` | only on a routine's run or a request (table above) | otherwise |
| `computer`, `manage` | never | |

### The prompt, a draft

The system prompt of a task on the bot's own item (`render_task`); the other sources change the "How it ends"
paragraph and add their own (routine, request):

```text
You are doing one item for the user, through Codync. The user talks to Codync from its phone, desktop or terminal
app, not in <agent>'s own CLI, so don't point them at its commands or shortcuts.

Item: everything you know about it is in the message below. You don't see the conversation it came from; if
something you need isn't there, look it up (memory, the files) before you ask.

Where: you work in <folder> [a git worktree of <project> on branch codync/<slug>, from its last commit: the user's
uncommitted changes are not in it]. Keep to it; don't change Codync settings to work elsewhere.

How it ends: your last message is your report. It goes to the bot that gave you this item, which tells the user, so
say what you did, what you found, what is left, and where (files, branch). When you need the user to choose, call
request_decision; to hand over a file or a page, send_file or publish_artifact.

Memory: what the user and the bot have kept is searchable with search_memory and read_memory; save what you learn
that should outlast this item with edit_memory.

Dev servers: <dev_servers::BRIEF>

Skills you can use when they fit the request:
- <name>: <description> (read <path>/SKILL.md before using it)

<KEEP>
```

The first message is the brief as today (`tasks::prompts::item_prompt`, without what it says now to override the
chat's prompt). A user's task gets the bot's profile instead ([above](#a-users-task-is-the-bot)).

### How a task is used

1. **Handed out.** In its chat the bot calls `add_work {title, brief, folder?, place?, after?}`; the item waits in
   the event box. (A user's task: `startThread`, no wait, §6.)
2. **Started.** The dispatcher has a seat: it makes the task bot (the bot's config with the item's setup), picks its
   folder or worktree (§4) and fills a new worktree, then opens a new session with `render_task` and the brief as
   the first message.
3. **Working.** The task uses the agent's tools in its folder. A choice or approval shows as a card in the bot's
   chat (a user's task: in the task), and the task waits on it, its seat lent to the next item.
4. **Watched.** The bot reads it with `work_status` and `recall_work`, corrects it with `tell_work`, asks with
   `ask_work` (proposed), restarts it with `steer_work replace` or ends it with `steer_work stop`. The user can
   open the task and write to it.
5. **Reported.** Its turn ends: the last message is the report; the bot gets a turn with it and tells the user (a
   user's task: it stays in the task). Its worktree is removed or kept (§4).
6. **Next or gone.** An item that follows it (`after`) goes on in the same task and session; otherwise the task
   ends (its agent stops, its transcript stays), and it is deleted seven days after its last item.

### No fork

- `sources::from_chat` goes; `dispatch::fork_source`, `tasks_fork`, `item.fork_from`, `item.fork_failed` and
  the actor's fork path in `agent/bot/session.rs` go with their tests and the `background-tasks.md` text about forks. An item
  that follows another (`after`) still goes on in that task's session.
- `capacity.chat_free` stays for the bot's own items: stopping the turn that added an item drops the item, and a
  routine's run or a room's turn shares the actor, so an item still starts once that turn ends. A user's task
  doesn't wait for it ([§6](#6-a-users-task-leaves-the-bots-queue)).
- The brief carries the weight: `add_work`'s `brief` description and `tasks::BRIEF` say the task knows nothing of
  the chat, so the brief has the goal, what is known, the user's words that matter and what done looks like. The
  user's words of the turn are already kept with the item (`words`, `notes`) and still go to the task.

### Host

- `render_task`, with a test per source for what is in and out (never `tasks::BRIEF`, the bot's name or its memory
  opening).
- `agent/bot/session.rs`: a task's new session renders `render_task` from its item; `context::adopt` and the
  snapshot bookkeeping take that text as they take the chat's.
- `Session::mcp_servers`: a task lists only its source's servers; the `chat` server started with `--task`
  lists only what a task may call, `refuse_chat_only` stays as the guard behind it.

### How to check

- Tokens: a new task's first-turn input tokens before and after, one Claude and one Codex task.
- Behavior: `task_tool_refused` (new event, by tool) falls to near zero; measured before the change.
- Quality: the same five briefs before and after; a task that asks for chat context it no longer has means
  `tasks::BRIEF` must ask for fuller briefs.

## 8. The bot watching its tasks

What the bot has today (`chat` server; for its own items, routine runs and requests, not a user's tasks after §6):

| To | Tool |
| --- | --- |
| See what each task does now | `work_status` (status, the task's activity line, why an item waits) |
| Read a task's steps | `recall_work {item}` (30 steps a page, `before` for the page before) |
| Correct its direction | `tell_work {item, text}` (after its running turn; `now` into it) |
| Start it again from a new brief | `steer_work replace` |
| End it | `steer_work stop` |
| Reorder | `steer_work first` / `now` |

Missing: a question with an answer. `tell_work`'s words are read, but the task's answer comes only in its report.

Proposed: `ask_work {item, question}` → `{answer}`. The question goes into the task's running turn as
`tell_work now` does, marked as the bot asking; the task answers with one short message that goes back to the bot
as the tool's result (not as the report) and goes on with its item. The bot's turn waits for the answer up to a time
limit (60 s), then gets `no answer yet` and the answer arrives later as a turn of its own, as a `message_bot` reply
does. Refused for an ended item, a user's task, a routine's run and a request. Analytics: `work_asked` with
`outcome: answered | late | refused`.

## Decisions (owner, 2026-11-24)

1. **Worktree mode per bot**: Off, When tasks run side by side (default), Always.
2. **Project list per bot.** A computer-wide list is a later step of the personal-agent plan.
3. **`add_work` takes any absolute path**; the bot's projects are what it proposes.
4. **Readable home names** (`bots/<name-slug>`): a separate change.
5. **No `minApp` raise**: an older app can add a project through its old picker but can't see the list.
6. **A user's task leaves the bot's queue and the bot's view** (§6).
7. **Tasks are told only their item**: the task profile, skills kept, memory looked up (§7). A task the user
   starts with a bot is the bot: its name, description, skills and memory opening, without the chat's paragraphs.
8. **No session fork** for any task (§7).
9. **The words**: bot, chat, item, brief, task, report, event box; not coordinator or worker.

## Open

- [An item names its place](#an-item-names-its-place), [filling a new worktree](#filling-a-new-worktree),
  [§5](#5-tasks-without-a-bot-get-a-folder-of-their-own) and [`ask_work`](#8-the-bot-watching-its-tasks): proposed,
  waiting for the owner.
- Can a user's task hand its result to the bot ("tell my bot about this")? A button that posts the task's result
  into the bot's chat would, without the bot seeing the box.
- Does a user's task want the bot's connectors, or only its skills and memory?
- Do the files of a task without a bot outlive it?

## Rollout

Each stage ships on its own, in this order; host-only stages need no client release beyond the version bump.

1. **Recap on a folder change.** Built (`a5fccdbf7`).
2. **Task profile and no fork** (§7). Host only. Built: a routine's run and another bot's request are told who the
   bot is, as a user's task is (they act as the bot, not on a brief the bot wrote), with only their own paragraph.
3. **A user's task leaves the queue** (§6). Host only. Built; `waitedMs` went on `work_finished` (there is no start
   event).
4. **Routine `folder`.** Host + clients (the routine editor). Built: any existing folder for now (projects come in 5);
   a run with one always works in a thread, and a folder that went fails the run.
5. **Projects**: `projects`, `add_work` `folder`, `worktreeMode`, side by side per repository, the Workspace line,
   the clients' Projects section, cleanup in the hourly look (§3, §4). Minor version, every client. Host built: as
   [background tasks: where a task works](../features/background-tasks.md#where-a-task-works) says. Beyond the plan: the box of tasks
   without a bot keeps the home folder as its `cwd` (it gets no projects; stage 7 then gave each of its threads a folder of its own, §5); a task in a worktree
   doesn't count toward side by side (its checkout is its own); the bot hears a change to its projects as a profile
   update; `projectGitRepos` (bools in `projects`' order) tells the clients which projects are repositories, and
   `gitRepo` means the default project for older apps; schema 14 stores `worktreeMode` in place of the bool.
   `market/verify.rs` keeps checking a connector in the bot's own folder, where the chat's agent starts it now.
   Clients built in the same change (the Projects section and Worktree mode on all four).
6. **Migration** (§3). Needs 4 and 5. Built with 5 (`hub/projects/migrate.rs`). A routine stamped with the old folder
   always runs in a thread, so on a bot that uses the computer or manages the team (whose runs were turns in its chat)
   it loses those tools: that is the price of keeping the folder, kept as the plan says.
7. **A task's place, filling a new worktree, threads without a bot's own folder, `ask_work`** (§4, §5, §8). Host
   and clients built, as [background tasks](../features/background-tasks.md) says: the desktop's new-thread place menu (Here / New worktree)
   and Own folder, and the Shared folders editor under each git project on the desktop, iPhone, Android and TUI. Beyond the plan: the place is kept in the task's `setup`, so a task
   tried again or replaced keeps it, and `add_work` answers `placeNote` when the place can't apply; the folder of a
   task without a bot is made as its task is added and goes when the task leaves the box (dropped, or a week after
   it ended), which is when its thread bot and transcript go; a shared folder is linked only into worktrees of its
   project's repository, at the project's place inside it, and the links Codync made don't keep an untouched
   worktree (each link is listed in the repository's `info/exclude`, so a task can't commit it); `ask_work` takes one question at a time a task, and a task that ends before it answers counts as
   `late`; a late answer is its own host turn (`Cmd::TaskAnswer`). `hello.threadPlace`, `hello.worktreeFill`,
   `hello.threadFolders` tell the clients.

Docs in the same stages: `docs/features/background-tasks.md`, `docs/architecture/conversation-runtime.md` (Work), the user
manual's bots and conversations pages, the site's mockups of bot settings.
