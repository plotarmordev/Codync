//! Threads without a bot ([threads without a bot](../../../../docs/features/background-tasks.md#threads-without-a-bot)):
//! the user runs an agent on its own, with the folder, approvals and model they choose. Their items wait in one box
//! of the computer's, a hidden bot that is never a bot to the user (`BotConfig::plain`), so they are steered,
//! stopped and listed as any task is. What the box's tasks run on is each one's `Setup`.

use super::items::Item;
use crate::LockExt;
use crate::hub::Hub;
use crate::store::{MAX_AGENTS, Store};
use anyhow::{Context, Result};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// What an app from before this box shows where it lists hidden bots.
const NAME: &str = "Agents";

/// Where the folders of threads without a bot are kept.
fn folders() -> PathBuf {
    crate::service::data_dir().join("threads")
}

/// The folder of its own a thread without a bot works in when the user picks none, for its item `item`: made as the
/// item is added, versioned as a bot's own folder is (`agent::versions`), never given a worktree
/// (`workspace::own_folder`), and removed with its task: when the user drops it, or `threadKeepDays` deletes it with
/// its transcript (`archive::prune`).
pub(super) fn own_folder(item: &str) -> Result<PathBuf> {
    // Only ids the host issued become folder names.
    uuid::Uuid::parse_str(item).context("not an id Codync issued")?;
    Ok(folders().join(item))
}

/// Whether `folder` is the own folder of a thread without a bot.
pub fn is_own_folder(folder: &str) -> bool {
    Path::new(folder).parent() == Some(folders().as_path())
}

/// `items` leave the box for good: the own folders of the threads without a bot among them go, with what is in them.
pub(super) fn forget_folders(items: &[Item]) {
    let mine = |item: &Item| own_folder(&item.id).ok().filter(|dir| item.setup.cwd.as_deref() == dir.to_str());
    let dirs: Vec<PathBuf> = items.iter().filter_map(mine).collect();
    if dirs.is_empty() {
        return;
    }
    let remove = move || {
        for dir in dirs.iter().filter(|dir| dir.exists()) {
            if let Err(error) = std::fs::remove_dir_all(dir) {
                tracing::warn!(dir = %dir.display(), error = %error, "couldn't remove a thread's own folder");
            }
        }
    };
    match tokio::runtime::Handle::try_current() {
        Ok(runtime) => drop(runtime.spawn_blocking(remove)),
        Err(_) => remove(),
    }
}

/// The box's id, if the computer has one.
fn find(store: &Store) -> Result<Option<String>> {
    Ok(store.bots()?.into_iter().find(|b| !b.deleted && b.config.plain).map(|b| b.config.id))
}

/// The computer's box for threads without a bot, made the first time one starts: hidden, in the user's home
/// folder, with no connectors, skills, shared memory or alerts of its own, and as many tasks at once as a bot
/// may have. The harness's own memory stays the agent's.
pub async fn owner(hub: &Arc<Hub>) -> Result<String> {
    if let Some(id) = find(&hub.store)? {
        return Ok(id);
    }
    let home = dirs::home_dir().context("no home folder")?.to_string_lossy().into_owned();
    let defaults = json!({"name": NAME, "connectors": [], "skills": [], "cwd": home});
    let mut cfg = crate::agent::new_bot::with_defaults(hub, defaults).await?;
    cfg.plain = true;
    cfg.hidden = true;
    cfg.notify = Some(false);
    cfg.offered_to_network = false;
    cfg.reachable_by_assistants = false;
    cfg.self_fallbacks = false;
    cfg.shared_memory = Some(false);
    cfg.harness_memory = true;
    cfg.max_agents = MAX_AGENTS;
    let hub = hub.clone();
    tokio::task::spawn_blocking(move || {
        // Two threads started at once make one box.
        static MAKING: Mutex<()> = Mutex::new(());
        let _g = MAKING.locked();
        if let Some(id) = find(&hub.store)? {
            return Ok(id);
        }
        let made = hub.create_bot(cfg)?;
        made["id"].as_str().map(str::to_owned).context("the box has no id")
    })
    .await?
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::BotConfig;

    #[test]
    fn a_thread_without_a_bot_works_in_its_own_folder_which_never_gets_a_worktree() {
        let item = uuid::Uuid::new_v4().to_string();
        let own = own_folder(&item).expect("a folder").to_string_lossy().into_owned();
        assert!(is_own_folder(&own) && !is_own_folder(&format!("{own}/sub")) && !is_own_folder("/tmp"));
        assert!(own_folder("../x").is_err(), "only ids become folder names");
        let task = BotConfig {
            plain: true,
            task_of: Some("box".into()),
            ..crate::chat::calls::context::tests::bot("task-1", "Agents", &own)
        };
        assert_eq!(crate::agent::workspace::own_folder(&task), Some(PathBuf::from(&own)), "versioned, no worktree");
        let picked = BotConfig { cwd: "/tmp".into(), ..task.clone() };
        assert!(crate::agent::workspace::own_folder(&picked).is_none(), "a folder the user picked is theirs");
        let bots = BotConfig { plain: false, ..task };
        assert!(crate::agent::workspace::own_folder(&bots).is_none(), "only a thread without a bot");
    }
}
