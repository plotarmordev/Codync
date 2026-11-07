//! Dictation on this computer: Underdog Bark 3.0, an English speech model, on transcribe.cpp in the
//! host's own process (Metal on the Mac's GPU). It loads on first use and stays loaded, so audio
//! never leaves the computer and no key is needed. Macs only.

use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

const MODEL: &str = "underdog-bark-3.0-Q8_0.gguf";
/// Pinned to the conversion of Bark 3.0 made while it was CC BY 4.0 (Conway's later revisions are non-commercial).
const MODEL_URL: &str = "https://huggingface.co/joshthomasdev/underdog-bark-3.0-gguf/resolve/cee1f12dfe250983ed4987f7b4a95f4577c2cb97/underdog-bark-3.0-Q8_0.gguf";
/// The model was measured with this much silence before the speech; without it, a first word spoken
/// right as the recording starts can be lost.
const LEAD_IN: usize = 4_000;

fn model_path() -> PathBuf {
    crate::service::data_dir().join("speech").join(MODEL)
}

/// What's missing for dictation on this computer, as the user should read it.
fn missing(model: &Path) -> Option<String> {
    if !cfg!(target_os = "macos") {
        return Some("Dictation on this computer needs a Mac.".to_owned());
    }
    (!model.is_file()).then(|| {
        format!(
            "This computer's speech model isn't installed. To dictate with it, download {MODEL_URL} to {}.",
            model.display()
        )
    })
}

/// `{ready, message}` for `localDictation`: whether dictation on this computer can run, and if not, what to install.
pub fn status() -> Value {
    let message = missing(&model_path());
    json!({"ready": message.is_none(), "message": message})
}

/// Speech to text for one utterance (a 16 kHz mono 16-bit WAV), on this computer.
pub async fn transcribe(wav: &[u8]) -> Result<Value> {
    let model = model_path();
    if let Some(message) = missing(&model) {
        bail!(message);
    }
    let pcm = samples(wav);
    let text = tokio::task::spawn_blocking(move || engine::run(&model, &pcm)).await??;
    Ok(json!({"text": text.trim()}))
}

/// The WAV's samples as the model takes them: floats in [-1, 1], after `LEAD_IN` of silence.
fn samples(wav: &[u8]) -> Vec<f32> {
    let start = wav.windows(4).position(|w| w == b"data").map_or(wav.len(), |i| i + 8);
    let mut pcm = vec![0.0; LEAD_IN];
    pcm.extend(
        wav[start.min(wav.len())..].as_chunks::<2>().0.iter().map(|b| f32::from(i16::from_le_bytes(*b)) / 32_768.0),
    );
    pcm
}

#[cfg(target_os = "macos")]
mod engine {
    use super::{Context, Path, Result, anyhow};
    use crate::LockExt;
    use std::sync::Mutex;
    use transcribe_cpp::{Model, RunOptions, Session};

    /// The loaded model, kept for every later dictation (loading reads 740 MB).
    static SESSION: Mutex<Option<Session>> = Mutex::new(None);

    pub fn run(model: &Path, pcm: &[f32]) -> Result<String> {
        let mut session = SESSION.locked();
        if session.is_none() {
            let loaded = Model::load(model).with_context(|| format!("couldn't load {}", model.display()))?;
            *session = Some(loaded.session()?);
        }
        let session = session.as_mut().ok_or_else(|| anyhow!("speech model not loaded"))?;
        Ok(session.run(pcm, &RunOptions::default())?.text)
    }
}

#[cfg(not(target_os = "macos"))]
mod engine {
    use super::{Path, Result};

    pub fn run(_: &Path, _: &[f32]) -> Result<String> {
        unreachable!("missing() turns dictation on this computer away off the Mac")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn samples_are_scaled_after_a_silent_lead_in() {
        let wav = super::super::wav(&[0x00, 0x40, 0x00, 0x80, 0xff, 0x7f], 16_000);
        let pcm = samples(&wav);
        assert_eq!(pcm.len(), LEAD_IN + 3);
        assert!(pcm[..LEAD_IN].iter().all(|&s| s == 0.0));
        assert_eq!(&pcm[LEAD_IN..], &[0.5, -1.0, 32_767.0 / 32_768.0]);
    }

    #[test]
    fn it_says_where_the_model_goes_until_it_is_there() {
        let dir = std::env::temp_dir().join(format!("codync-speech-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let model = dir.join(MODEL);
        let before = missing(&model).unwrap();
        std::fs::write(&model, b"").unwrap();
        let after = missing(&model);
        std::fs::remove_dir_all(&dir).unwrap();
        if cfg!(target_os = "macos") {
            assert!(before.contains(MODEL_URL) && before.contains(&model.display().to_string()), "{before}");
            assert_eq!(after, None);
        } else {
            assert_eq!(before, "Dictation on this computer needs a Mac.");
            assert_eq!(after.as_deref(), Some(before.as_str()));
        }
    }
}
