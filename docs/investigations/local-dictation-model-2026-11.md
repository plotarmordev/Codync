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

## Choice

Whisper large-v3-turbo on whisper.cpp's `whisper-server`, started by the host on first use and kept loaded. MLX is as accurate and a little faster on this Mac, but it needs Python and a package manager on the user's computer and runs only on Apple silicon; whisper.cpp is one native binary (Homebrew `whisper-cpp` on the Mac, packages on Linux and Windows) that the host talks to over loopback HTTP.
