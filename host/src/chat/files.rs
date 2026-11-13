//! Immutable snapshots explicitly published by a bot, independent of its workspace.
use crate::chat::uploads::MAX_FILE;
use anyhow::{Context as _, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use std::fs::{File, OpenOptions};
use std::io::{Read as _, Seek as _, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

pub const CHUNK_SIZE: usize = 384 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SharedFile {
    pub id: String,
    pub name: String,
    pub size: u64,
    pub sha256: String,
    /// A sound the chat plays in place (a voice note), as opposed to a file to download. Stamped once by the host so
    /// every client follows one rule; missing (an older host) means a plain file card.
    #[serde(default, rename = "playsInChat")]
    pub plays_in_chat: bool,
    /// A playable sound's length (milliseconds), read from its headers when shared, so a player shows its bar and
    /// clock before the sound is fetched; missing when unknown (an older host, an unreadable header).
    #[serde(default, rename = "durationMs", skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
}

/// The largest sound a chat fetches whole to play.
const PLAYABLE_BYTES: u64 = 20 * 1024 * 1024;

fn plays_in_chat(name: &str, size: u64) -> bool {
    let extension = name.rsplit_once('.').map(|(_, ext)| ext.to_ascii_lowercase());
    matches!(extension.as_deref(), Some("mp3" | "m4a" | "wav" | "aac")) && size <= PLAYABLE_BYTES
}

/// A sound's length from its headers; the stored copy has no extension, so its format is sniffed from the bytes. An
/// MP3 or AAC stream without a length header gets an estimate; the player swaps in the decoded length once loaded.
fn duration_ms(path: &Path) -> Option<u64> {
    use lofty::file::AudioFile as _;
    let file = lofty::probe::Probe::open(path).ok()?.guess_file_type().ok()?.read().ok()?;
    let millis = file.properties().duration().as_millis();
    (millis > 0).then(|| u64::try_from(millis).ok()).flatten()
}

/// Fills in the length of playable sounds shared before the host read lengths, so their players show a bar and clock
/// from the first frame. Runs once per database (`kv` `files.duration`); a changed message gets a new rev, keeping its
/// content rev so a message the user read stays read, and clients fetch it again.
pub fn backfill_durations(store: &crate::store::Store) -> Result<()> {
    const DONE: &str = "files.duration";
    if store.kv_get(DONE).is_some() {
        return Ok(());
    }
    for entry in store.unstamped_sounds()? {
        let root = root(&entry.bot_id);
        let mut data = entry.data.clone();
        let mut changed = false;
        for file in data["files"].as_array_mut().into_iter().flatten() {
            if file["playsInChat"] != true || !file["durationMs"].is_null() {
                continue;
            }
            let Some(id) = file["id"].as_str() else { continue };
            let Ok(path) = path(&root, id) else { continue };
            if let Some(millis) = duration_ms(&path) {
                file["durationMs"] = millis.into();
                changed = true;
            }
        }
        if changed {
            if data["contentRev"].is_null() {
                data["contentRev"] = entry.rev.into();
            }
            store.update_entry(&entry.id, &data)?;
        }
    }
    store.kv_set(DONE, "1")
}

/// Removes an unpublished snapshot on every failure, including a dropped actor.
pub struct Snapshot {
    pub meta: SharedFile,
    path: PathBuf,
    committed: bool,
}

impl Snapshot {
    pub fn commit(&mut self) {
        self.committed = true;
    }
}

impl Drop for Snapshot {
    fn drop(&mut self) {
        if !self.committed {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

pub fn root(bot: &str) -> PathBuf {
    crate::service::data_dir().join("bots").join(bot).join("files")
}

fn path(root: &Path, id: &str) -> Result<PathBuf> {
    let id = uuid::Uuid::parse_str(id).context("invalid file id")?;
    Ok(root.join(id.to_string()))
}

/// Display names never control storage paths; hidden and extensionless files are valid.
pub fn name(value: &str) -> Result<String> {
    ensure!(!value.is_empty() && value.len() <= 200, "file name must be 1–200 bytes");
    ensure!(
        value != "." && value != ".." && !value.contains(['/', '\\', ':']) && !value.chars().any(char::is_control),
        "invalid file name"
    );
    Ok(value.to_owned())
}

pub(crate) fn open_regular(path: &Path) -> Result<File> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    ensure!(std::fs::symlink_metadata(path)?.file_type().is_file(), "only regular files can be shared");
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt as _;
        options.custom_flags(0x0020_0000); // FILE_FLAG_OPEN_REPARSE_POINT
    }
    let file = options.open(path).context("opening the file")?;
    ensure!(file.metadata()?.is_file(), "only regular files can be shared");
    Ok(file)
}

/// Blocking, bounded-memory copy. Call on a blocking worker, never the bot actor.
pub fn snapshot(
    root: &Path,
    cwd: &Path,
    source: &str,
    display: Option<&str>,
    cancelled: &AtomicBool,
) -> Result<Snapshot> {
    let original = cwd.join(source);
    let source = original.canonicalize().context("finding the file")?;
    let display = match display {
        Some(value) => name(value)?,
        None => name(original.file_name().and_then(|v| v.to_str()).context("file name is not UTF-8; provide name")?)?,
    };
    let mut input = open_regular(&source)?;
    let before = input.metadata()?;
    ensure!(before.len() <= MAX_FILE, "file exceeds 100 MiB");
    std::fs::create_dir_all(root).context("creating the shared file folder")?;
    let id = uuid::Uuid::new_v4().to_string();
    let final_path = path(root, &id)?;
    let partial = final_path.with_extension("partial");
    let mut owned = Snapshot {
        meta: SharedFile { id, name: display, size: 0, sha256: String::new(), plays_in_chat: false, duration_ms: None },
        path: partial.clone(),
        committed: false,
    };
    let mut output = OpenOptions::new().write(true).create_new(true).open(&partial)?;
    let (size, digest) = copy(&mut input, &mut output, cancelled)?;
    owned.meta.size = size;
    let after = input.metadata()?;
    ensure!(
        owned.meta.size == before.len() && after.len() == before.len() && after.modified()? == before.modified()?,
        "source file changed while sharing; retry"
    );
    output.sync_all()?;
    drop(output);
    ensure!(!cancelled.load(Ordering::Acquire), "file sharing cancelled");
    std::fs::rename(&partial, &final_path)?;
    owned.path = final_path;
    owned.meta.sha256 = digest;
    owned.meta.plays_in_chat = plays_in_chat(&owned.meta.name, owned.meta.size);
    if owned.meta.plays_in_chat {
        owned.meta.duration_ms = duration_ms(&owned.path);
    }
    Ok(owned)
}

fn copy(
    input: &mut impl std::io::Read,
    output: &mut impl std::io::Write,
    cancelled: &AtomicBool,
) -> Result<(u64, String)> {
    let mut hash = Sha256::new();
    let mut size = 0;
    let mut buffer = vec![0; CHUNK_SIZE];
    loop {
        ensure!(!cancelled.load(Ordering::Acquire), "file sharing cancelled");
        let count = input.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        size += count as u64;
        ensure!(size <= MAX_FILE, "file exceeds 100 MiB");
        output.write_all(&buffer[..count])?;
        hash.update(&buffer[..count]);
    }
    Ok((size, digest_hex(&hash.finalize())))
}

pub fn digest_hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut value = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        // Formatting into a String cannot fail.
        let _ = write!(value, "{byte:02x}");
    }
    value
}

pub fn read(root: &Path, meta: &SharedFile, offset: u64) -> Result<Vec<u8>> {
    ensure!(meta.size <= MAX_FILE && offset <= meta.size, "invalid file offset or size");
    read_chunk(&path(root, &meta.id)?, meta.size, offset, "shared file is missing or damaged")
}

/// Up to `CHUNK_SIZE` bytes of the stored file `path` from `offset` (at most `size`); fails with `damaged` unless the
/// file is `size` bytes long.
pub(crate) fn read_chunk(path: &Path, size: u64, offset: u64, damaged: &str) -> Result<Vec<u8>> {
    let mut file = open_regular(path)?;
    ensure!(file.metadata()?.len() == size, "{damaged}");
    file.seek(SeekFrom::Start(offset))?;
    let mut data = vec![0; usize::try_from((size - offset).min(CHUNK_SIZE as u64))?];
    file.read_exact(&mut data)?;
    Ok(data)
}

#[cfg(test)]
mod tests;
