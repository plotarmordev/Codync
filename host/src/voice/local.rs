//! Dictation on this computer: whisper.cpp's `whisper-server` with Whisper large-v3-turbo,
//! started on first use and kept loaded (a cold start reads 1.6 GB). It listens on loopback
//! only, so audio never leaves the computer and no key is needed.

use super::{TRANSCRIBE_HINT, multipart, names_hint, spoken_names};
use crate::store::Store;
use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::time::Duration;
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

const SERVER: &str = "whisper-server";
const MODEL: &str = "ggml-large-v3-turbo.bin";
const MODEL_URL: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo.bin";
const LOAD_TIMEOUT: Duration = Duration::from_mins(1);
const TRANSCRIBE_TIMEOUT: Duration = Duration::from_mins(2);
/// Whisper drifts into English on Chinese speech with an English-only prompt; a Chinese line keeps it
/// in the speaker's language, and in Traditional script (the phones convert to the reader's).
const CHINESE_HINT: &str = "請保留說話者的語言與繁體中文。";
/// Whisper makes up words ("Thank you.") for silence; a recording whose loudest 100 ms stays under
/// this RMS (-45 dBFS; room noise is about -60, speech -30 to -10) has no speech to transcribe.
const SPEECH_RMS: u64 = 184;
/// 100 ms at 16 kHz.
const WINDOW: usize = 1600;

/// The running server and its port.
static RUNNING: Mutex<Option<(Child, u16)>> = Mutex::const_new(None);

fn model_path() -> PathBuf {
    crate::service::data_dir().join("speech").join(MODEL)
}

/// What's missing for dictation on this computer, as the user should read it: `server` is whether
/// `whisper-server` is on the PATH, `model` where the model file belongs.
fn missing(server: bool, model: &Path) -> Option<String> {
    let has_model = model.is_file();
    if server && has_model {
        return None;
    }
    let mut steps = Vec::new();
    if !server {
        steps.push(if cfg!(target_os = "macos") {
            "install whisper.cpp (brew install whisper-cpp)".to_owned()
        } else {
            format!("install whisper.cpp so {SERVER} is on the PATH")
        });
    }
    if !has_model {
        steps.push(format!("download {MODEL_URL} to {}", model.display()));
    }
    Some(format!("This computer's speech model isn't installed. To dictate with it, {}.", steps.join(", then ")))
}

fn missing_here() -> Option<String> {
    missing(crate::agent::backends::on_path(SERVER), &model_path())
}

/// `{ready, message}` for `localDictation`: whether dictation on this computer can run, and if not, what to install.
pub fn status() -> Value {
    let message = missing_here();
    json!({"ready": message.is_none(), "message": message})
}

/// Stops the speech server, if it runs, and waits for it to exit: the host calls this as it shuts
/// down, since a static's child is never dropped (so `kill_on_drop` never fires) when the process exits.
pub async fn stop() {
    if let Some((mut child, _)) = RUNNING.lock().await.take()
        && let Err(error) = child.kill().await
    {
        tracing::warn!(%error, "couldn't stop the speech server");
    }
}

/// The server's port, starting it (and waiting for the model to load) when it isn't running.
async fn port() -> Result<u16> {
    let mut running = RUNNING.lock().await;
    if let Some((child, port)) = running.as_mut()
        && child.try_wait()?.is_none()
    {
        return Ok(*port);
    }
    if let Some(message) = missing_here() {
        bail!(message);
    }
    // A free loopback port: the OS picks one, and the server takes it over once this listener closes.
    let port = std::net::TcpListener::bind(("127.0.0.1", 0))?.local_addr()?.port();
    let program = crate::agent::backends::which(SERVER).context("whisper-server went missing")?;
    let mut child = Command::new(program)
        .args(["--host", "127.0.0.1", "--port", &port.to_string(), "--language", "auto", "--no-timestamps"])
        .arg("--model")
        .arg(model_path())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .context("couldn't start whisper-server")?;
    let deadline = tokio::time::Instant::now() + LOAD_TIMEOUT;
    while tokio::net::TcpStream::connect(("127.0.0.1", port)).await.is_err() {
        if child.try_wait()?.is_some() {
            bail!("This computer's speech model didn't start. Download it again: {MODEL_URL}");
        }
        if tokio::time::Instant::now() > deadline {
            bail!("This computer's speech model took too long to load.");
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    *running = Some((child, port));
    Ok(port)
}

/// The client that talks to the speech server: never through a proxy (`HTTP_PROXY` would get the
/// recording) and never redirected off this computer.
fn loopback() -> &'static reqwest::Client {
    static CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
        reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("TLS setup failed: install the ca-certificates package")
    });
    &CLIENT
}

/// Speech to text for one utterance (a 16 kHz mono WAV), on this computer.
pub async fn transcribe(store: &Store, wav: &[u8]) -> Result<Value> {
    if !voiced(wav) {
        return Ok(json!({"text": ""}));
    }
    let port = port().await?;
    let prompt = format!("{TRANSCRIBE_HINT} {} {CHINESE_HINT}", names_hint(&spoken_names(store)));
    let (boundary, body) = multipart(&[("prompt", &prompt), ("temperature", "0"), ("response_format", "json")], wav);
    let res = loopback()
        .post(format!("http://127.0.0.1:{port}/inference"))
        .header("Content-Type", format!("multipart/form-data; boundary={boundary}"))
        .body(body)
        .timeout(TRANSCRIBE_TIMEOUT)
        .send()
        .await
        .map_err(|_| anyhow!("This computer's speech model stopped answering."))?;
    let v: Value = res.error_for_status()?.json().await?;
    Ok(json!({"text": spoken(v["text"].as_str().unwrap_or_default())}))
}

/// The transcript without whisper.cpp's markers for silence and sounds (`[BLANK_AUDIO]`, `(music)`).
fn spoken(text: &str) -> String {
    let marker = |w: &str| (w.starts_with('[') && w.ends_with(']')) || (w.starts_with('(') && w.ends_with(')'));
    let text = text.trim();
    if marker(text) { String::new() } else { text.to_owned() }
}

/// Whether any 100 ms of a 16 kHz 16-bit WAV is loud enough to be speech.
fn voiced(wav: &[u8]) -> bool {
    let Some(start) = wav.windows(4).position(|w| w == b"data").map(|i| i + 8) else {
        return true;
    };
    let samples: Vec<i16> =
        wav[start.min(wav.len())..].as_chunks::<2>().0.iter().map(|b| i16::from_le_bytes(*b)).collect();
    samples.chunks(WINDOW).any(|w| {
        let energy: u64 = w.iter().map(|&s| u64::from(s.unsigned_abs()).pow(2)).sum();
        energy >= SPEECH_RMS.pow(2) * w.len() as u64
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silence_markers_are_no_words() {
        assert_eq!(spoken(" [BLANK_AUDIO]\n"), "");
        assert_eq!(spoken("(music)"), "");
        assert_eq!(spoken(" Run cargo test (twice).\n"), "Run cargo test (twice).");
    }

    #[test]
    fn silence_and_room_noise_are_not_sent_to_the_model() {
        let wav = |level: i16| {
            super::super::wav(
                &(0..16_000).flat_map(|i| if i % 2 == 0 { level } else { -level }.to_le_bytes()).collect::<Vec<u8>>(),
                16_000,
            )
        };
        assert!(!voiced(&wav(0)));
        assert!(!voiced(&wav(30)));
        assert!(voiced(&wav(3_000)));
    }

    #[test]
    fn it_says_what_to_install_until_both_are_there() {
        let dir = std::env::temp_dir().join(format!("codync-speech-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let model = dir.join(MODEL);
        let neither = missing(false, &model).unwrap();
        assert!(neither.contains("install whisper.cpp") && neither.contains(MODEL_URL), "{neither}");
        let no_model = missing(true, &model).unwrap();
        assert!(
            !no_model.contains("install whisper.cpp") && no_model.contains(&model.display().to_string()),
            "{no_model}"
        );
        std::fs::write(&model, b"").unwrap();
        assert!(!missing(false, &model).unwrap().contains(MODEL_URL));
        assert_eq!(missing(true, &model), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
