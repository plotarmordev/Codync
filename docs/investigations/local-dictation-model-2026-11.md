# Local dictation: which model and runtime (November 2026)

The **This computer** dictation engine transcribes on the computer running the host. This is how its model and runtime were chosen.

## Setup

- Mac: Apple M5 Ultra, 256 GB.
- Clips: synthetic speech from macOS `say` (English: Eddy; Traditional Chinese: Meijia), 16 kHz mono, padded with silence to exactly 10 s and 60 s, the way a recording ends after a pause. The texts mix code terms (`cargo clippy`, `useEffect`, Kotlin coroutine, SwiftUI, async await, JSON, worktree, `npm run typecheck`) with bot names (Codync, Builder, Scout).
- As the clients send it: 20 s pieces, three at a time; the time is from the first piece sent to the last text back, with the model already loaded.
- The host's prompt: its transcription hint plus the names hint (`Names that may come up: Codync, Builder, Scout.`).
- Error rate: English word error rate; Chinese character error rate (each Han character a token, Latin words whole) after converting to Traditional script, since the phones show the reader's script anyway.
- Parakeet (NVIDIA) was not measured: its multilingual models don't cover Chinese.

## Results

Error rate (%) / seconds from stop to text, Whisper large-v3-turbo:

| Runtime, prompt | EN 10 s | EN 60 s | ZH 10 s | ZH 60 s |
| --- | --- | --- | --- | --- |
| whisper.cpp `whisper-server`, hint + names + Chinese line | 12.0 / 0.18 | 18.7 / 0.65 | 10.7 / 0.19 | 17.1 / 1.34 |
| whisper.cpp `whisper-server`, hint + names (English only) | 12.0 / 0.19 | 7.7 / 0.69 | 14.3 / 0.20 | 70.1 / 1.03 |
| whisper.cpp `whisper-cli`, one process per piece | 12.0 / 0.95 | 7.7 / 5.25 | 10.7 / 0.77 | 16.6 / 1.92 |
| MLX (`mlx-whisper`), hint + names + Chinese line | 12.0 / 0.14 | 8.4 / 0.86 | 10.7 / 0.14 | 19.3 / 0.58 |
| MLX, hint + names (English only) | 12.0 / 0.13 | 9.0 / 0.52 | 10.7 / 0.14 | 59.9 / 3.67 |

What the numbers show:

- With an English-only prompt, a Chinese piece after the first drifts into invented English (60–70% errors). The Chinese line in the prompt fixes it, so the host always sends it.
- That line costs English: in one 60 s English piece whisper.cpp wrote "Codync" as "可Dinc" (18.7% against 7.7%). Chinese dictation is the bigger failure, so the line stays.
- Starting `whisper-cli` for each piece loads the 1.6 GB model each time: 5 s for a 60 s clip. A resident server answers in about a second.
- The remaining errors are the same everywhere: code terms read as words ("use effect", "cut linker routing"), and `Scout` heard as "Spot" in Chinese speech.
- Through the host's `voiceTranscribe` itself, the first dictation after the host starts takes about 4 s more (the server starts and loads the model); a warm 10 s clip takes 0.2 s.
- On silence the server's `no_speech_prob` stays near 0 and it writes "Thank you.", "you" or "."; the host checks loudness instead and skips silent pieces.

## Choice

Whisper large-v3-turbo on whisper.cpp's `whisper-server`, started by the host on first use and kept loaded. MLX is as accurate and a little faster on this Mac, but it needs Python and a package manager on the user's computer and runs only on Apple silicon; whisper.cpp is one native binary (Homebrew `whisper-cpp` on the Mac, packages on Linux and Windows) that the host talks to over loopback HTTP.

## Underdog Bark 3.0 (English only)

[Underdog Bark 3.0](https://huggingface.co/ConwayResearch/Underdog-Bark-3.0) is a 163 MB English model fine-tuned from NVIDIA's Parakeet TDT 0.6B v3, with a lower published word error rate than Whisper large-v3-turbo on the Open ASR Leaderboard's sets (4.94% against 6.58%). It was measured as an English engine beside Whisper, on the same English clips and pieces as above; Whisper was measured again in the same session.

- Bark: the model card's MLX runtime (`bark_mlx.py`, mlx 0.32.2, mlx-audio 0.5.3, fp16 weights, 0.25 s of leading silence). It takes no prompt, so the names hint can't reach it. MLX runs one piece at a time.
- Warm: the model already loaded. Cold: from starting the process (Bark: Python, imports and model load; Whisper: `whisper-server` start and model load) to the 10 s clip's text, the model file already in the disk cache, median of three.

Word error rate (%) / seconds from stop to text:

| Engine | EN 10 s | EN 60 s | Cold, 10 s |
| --- | --- | --- | --- |
| Bark 3.0, MLX | 12.0 / 0.08 | 12.9 / 0.39 | 0.94 |
| Whisper large-v3-turbo, `whisper-server`, hint + names (English only) | 12.0 / 0.19 | 7.1 / 0.71 | 1.02 |
| Whisper large-v3-turbo, `whisper-server`, hint + names + Chinese line (the host today) | 12.0 / 0.20 | 18.7 / 0.69 | — |

What the numbers show:

- Bark is not more accurate than Whisper on English: the same on the 10 s clip, and 12.9% against 7.1% on the 60 s clip when Whisper gets an English prompt. Both read code terms as words ("use effect", "Scott"), but Bark also misses what Whisper gets right: "Codync" as "coding" (the names hint fixes it in Whisper), "Kotlin coroutine" as "cotlin colo routine", "SwiftUI" as "swift two i".
- It beats the host's Whisper today (18.7%) only on the one 60 s clip where the Chinese line made Whisper drop a sentence and write "可Dinc"; on the 10 s clip the two are the same.
- Its text loses casing and punctuation partway through the 60 s clip (after the first 20 s piece), which the error rate, ignoring both, doesn't count.
- It is about twice as fast warm (0.39 s against 0.71 s for 60 s); cold, both answer in about a second. Its first run unpacks `model.bark` into a 1.2 GB fp16 folder, which takes 22 s.
- Licence: the model is CC BY-NC 4.0 (non-commercial; commercial use needs Conway's licence), not CC-BY-4.0 like the Parakeet model it's built on. Third-party ONNX, GGUF and Core ML copies tagged CC-BY-4.0 derive from it and don't change that.
- Runtime: Conway's own runtimes are Python (PyTorch, MLX, Core ML through `coremltools`); Underdog's native Swift runtime isn't published. A native one would mean a second engine beside whisper.cpp (for example sherpa-onnx with a third-party ONNX export).

Not adopted: Bark doesn't beat Whisper on English, can't take the names hint, and it's licensed for non-commercial use only. The host keeps Whisper large-v3-turbo for every language.
