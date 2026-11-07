//! Dictation on this computer: whisper.cpp's `whisper-server` with Whisper large-v3-turbo,
//! started on first use and kept loaded (a cold start reads 1.6 GB). It listens on loopback
//! only, so audio never leaves the computer and no key is needed.

use super::{TRANSCRIBE_HINT, multipart, names_hint, spoken_names};
use crate::store::Store;
use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Value, json};
use std::path::PathBuf;
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

/// The running server and its port.
static RUNNING: Mutex<Option<(Child, u16)>> = Mutex::const_new(None);

fn model_path() -> PathBuf {
    crate::service::data_dir().join("speech").join(MODEL)
}

/// What's missing for dictation on this computer, as the user should read it.
fn missing() -> Option<String> {
    let model = model_path();
    let server = crate::agent::backends::on_path(SERVER);
    if server && model.is_file() {
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
    if !model.is_file() {
        steps.push(format!("download {MODEL_URL} to {}", model.display()));
    }
    Some(format!("This computer's speech model isn't installed. To dictate with it, {}.", steps.join(", then ")))
}

/// `{ready, message}` for `voiceStatus`: whether dictation on this computer can run, and if not, what to install.
pub fn status() -> Value {
    let message = missing();
    json!({"ready": message.is_none(), "message": message})
}

/// The server's port, starting it (and waiting for the model to load) when it isn't running.
async fn port() -> Result<u16> {
    let mut running = RUNNING.lock().await;
    if let Some((child, port)) = running.as_mut()
        && child.try_wait()?.is_none()
    {
        return Ok(*port);
    }
    if let Some(message) = missing() {
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

/// Speech to text for one utterance (a 16 kHz mono WAV), on this computer.
pub async fn transcribe(store: &Store, wav: &[u8]) -> Result<Value> {
    let port = port().await?;
    let prompt = format!("{TRANSCRIBE_HINT} {} {CHINESE_HINT}", names_hint(&spoken_names(store)));
    let (boundary, body) = multipart(&[("prompt", &prompt), ("temperature", "0"), ("response_format", "json")], wav);
    let res = crate::http()
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
    fn without_the_model_it_says_what_to_install() {
        let status = status();
        assert_eq!(status["ready"], false);
        let message = status["message"].as_str().unwrap();
        assert!(message.contains(MODEL_URL) && message.contains(&model_path().display().to_string()), "{message}");
    }
}
