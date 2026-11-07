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

## First choice

Whisper large-v3-turbo on whisper.cpp's `whisper-server`, started by the host on first use and kept loaded. MLX is as accurate and a little faster on this Mac, but it needs Python and a package manager on the user's computer and runs only on Apple silicon; whisper.cpp is one native binary (Homebrew `whisper-cpp` on the Mac, packages on Linux and Windows) that the host talks to over loopback HTTP. Underdog Bark 3.0 replaced it ([below](#choice-english-only-on-bark-30)).

## Underdog Bark 3.0 (English only)

[Underdog Bark 3.0](https://huggingface.co/ConwayResearch/Underdog-Bark-3.0) by Conway Research is an English model fine-tuned from NVIDIA's Parakeet TDT 0.6B v3, with a lower published word error rate than Whisper large-v3-turbo on the Open ASR Leaderboard's sets (4.94% against 6.58%). It was measured as an English engine beside Whisper, on the same English clips and pieces as above; Whisper was measured again in the same session.

- Bark, MLX: the model card's runtime (`bark_mlx.py`, mlx 0.32.2, mlx-audio 0.5.3, fp16 weights, 0.25 s of leading silence). MLX runs one piece at a time.
- Bark, transcribe.cpp: [transcribe.cpp](https://github.com/handy-computer/transcribe.cpp) 0.4.1 through its Rust crate, Metal, the [Q8_0 GGUF](https://huggingface.co/joshthomasdev/underdog-bark-3.0-gguf/tree/cee1f12dfe250983ed4987f7b4a95f4577c2cb97) (740 MB) converted from Bark's CC BY 4.0 revision, 0.25 s of leading silence, one piece at a time.
- Bark takes no prompt: transcribe.cpp reports no prompt or vocabulary support for it, so the names hint can't reach it.
- Warm: the model already loaded. Cold: from starting the process (Bark on MLX: Python, imports and model load; Whisper: `whisper-server` start and model load) or, for Bark on transcribe.cpp, from loading the model in process, to the 10 s clip's text, the model file already in the disk cache, median of three.

Word error rate (%) / seconds from stop to text:

| Engine | EN 10 s | EN 60 s | Cold, 10 s |
| --- | --- | --- | --- |
| Bark 3.0, MLX | 12.0 / 0.08 | 12.9 / 0.39 | 0.94 |
| Bark 3.0, transcribe.cpp (Q8_0) | 12.0 / 0.08 | 12.9 / 0.34 | 0.25 |
| Whisper large-v3-turbo, `whisper-server`, hint + names (English only) | 12.0 / 0.19 | 7.1 / 0.71 | 1.02 |
| Whisper large-v3-turbo, `whisper-server`, hint + names + Chinese line (the host before Bark) | 12.0 / 0.20 | 18.7 / 0.69 | — |

What the numbers show:

- Bark is not more accurate than Whisper on English: the same on the 10 s clip, and 12.9% against 7.1% on the 60 s clip when Whisper gets an English prompt. Both read code terms as words ("use effect", "Scott"), but Bark also misses what Whisper gets right: "Codync" as "coding" (the names hint fixes it in Whisper), "Kotlin coroutine" as "cotlin colo routine", "SwiftUI" as "swift two i".
- It beats the host's earlier Whisper (18.7%) only on the one 60 s clip where the Chinese line made Whisper drop a sentence and write "可Dinc"; on the 10 s clip the two are the same.
- Its text loses casing and punctuation: partway through the 60 s clip on MLX (after the first 20 s piece), and from the start on transcribe.cpp's Q8_0. The error rate ignores both.
- It is about twice as fast warm (0.34–0.39 s against 0.71 s for 60 s). Cold, MLX and Whisper answer in about a second; transcribe.cpp loads the model in the host's process and answers in 0.25 s. MLX's first run unpacks `model.bark` into a 1.2 GB fp16 folder, which takes 22 s.
- On the 10 s and 60 s silence and room-noise clips it writes nothing, but a short recording is different: 0.5–1 s of digital silence, or of white noise anywhere from -70 to -36 dBFS, often comes back as "Yeah." or "Okay.". Its token confidence doesn't separate these (0.76–0.85 on the first token, against 0.91 or more on quiet real words), so the host keeps Whisper's loudness check: a recording whose loudest 100 ms stays under -45 dBFS is not sent to the model. Speech peaking at -42 dBFS still transcribes; steady noise louder than -45 dBFS still reaches the model and can come back as one word.
- Licence: Conway's repository switched to CC BY-NC 4.0 on 5 Nov 2026; revision `85db2ad18ab4` (15 Oct) and the GGUF converted from it (21 Oct) are CC BY 4.0, which can't be withdrawn from copies already released.
- Runtime: Conway's own runtimes are Python (PyTorch, MLX, Core ML through `coremltools`). transcribe.cpp is native C/C++ on ggml (MIT), with a Rust crate that builds it from source with CMake (Metal on the Mac); it runs Bark's GGUF unchanged.

One dictation in the style of a Codync user's (a 20 s English clip, macOS `say` with Samantha: "Okay Builder, the dictation pill on the iPhone cuts off the timer, can you fix that and push a new build to TestFlight? … open the PR against dev when it's green."):

| Engine | Word error rate (%) | Warm seconds |
| --- | --- | --- |
| Bark 3.0, transcribe.cpp (Q8_0) | 3.4 | 0.13 |
| Whisper large-v3-turbo, hint + names (English only) | 3.4 | 0.26 |
| Whisper large-v3-turbo, hint + names + Chinese line | 5.2 | 0.27 |

Both miss "TestFlight" ("test flight"); Bark also writes "pr" and "iphone" in lowercase, Whisper writes "Cargo Clippy".

## Choice: English only, on Bark 3.0

Dictation on this computer became English only: Underdog Bark 3.0 (the CC BY 4.0 Q8_0 GGUF) on transcribe.cpp, linked into the host and run in the host's process on Metal. No server, port, Python or package to install; the user downloads the model file. On English speech it matches the host's Whisper on short dictation and is twice as fast, but on the long code-heavy clip it makes more errors than Whisper with an English prompt (12.9% against 7.1%), can't take the names hint, and writes no capitals or punctuation. Only Macs build it in (the host's Linux builds are musl, with no C++ toolchain for ggml), so other hosts answer that dictation on this computer needs a Mac.
