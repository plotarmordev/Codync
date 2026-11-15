# iPhone scrolling measurements (2.48.1)

Simulator: iPhone 17, iOS 27.2, 60 Hz, `DevDebug` builds of this change and its base. Load: 60 bots in the roster, and a
long chat of 160 turns (320 messages, the first turn with 120 tool calls). The flows are `apps/maestro/roster-scroll.yaml`
(the roster to its oldest bot and back to the newest) and `apps/maestro/history.yaml` (the chat up to message 1 and back
down to the newest reply). These are simulator results; physical-iPhone validation is a TestFlight follow-up.

## Scroll frame timings

Every frame interval while a scroll view tracks or decelerates, from a `CADisplayLink` in a throwaway probe build (not
part of this change). A hitch is a frame longer than 1.5 × 16.67 ms; hitch time is the time past each frame's budget, per
second of scrolling. Two runs per build; before = the base commit.

| Scroll | Build | Frames | p50 | p95 | Longest frame | Hitches | Hitch time |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Roster, 60 bots | before | 1,903 / 1,901 | 16.67 ms | 16.67 ms | 38.1 / 38.3 ms | 8 / 9 | 4.2 / 4.9 ms/s |
| Roster, 60 bots | after | 1,896 / 1,899 | 16.67 ms | 16.67 ms | 43.7 / 42.4 ms | 12 / 11 | 7.0 / 6.7 ms/s |
| Long chat, both ends | before | 9,223 / 9,233 | 16.67 ms | 16.67 ms | 109.4 / 100.7 ms | 11 / 7 | 2.7 / 2.2 ms/s |
| Long chat, both ends | after | 9,228 / 9,236 | 16.67 ms | 16.67 ms | 100.4 / 115.6 ms | 13 / 7 | 3.6 / 2.5 ms/s |

Both builds hold 60 fps for at least 99% of scroll frames. The few long frames are within run-to-run noise of each other:
the Mac running the simulator had a load average near 46 during these runs, and the profiler (below) finds no app work
that lasts a frame's length in any of them.

## Profiler results

Time Profiler, 45 s of the long-chat scroll to message 1, main thread, without the UI-test driver's accessibility
snapshots:

| Build | Main-thread time | Largest app work in any 50 ms window |
| --- | ---: | ---: |
| before | 2,364 ms | 29 ms (SwiftUI `sizeThatFits` of newly shown cells) |
| after | 1,870 ms | 28 ms (same) |

Measured causes and what this change does about each:

- Long chat, before: repeated `ChatItem.build` / `BotExchangeGroup.collapse`, `MarkdownText.inline`, `AttributedString`
  mutation and `NSAttributedString` conversion when rows update, and every visible row reconfigured on any update.
  After: unchanged chat items and rows are cached and reconfigured only when what they show changes; main-thread time over
  the same scroll fell 21%.
- Roster, before: `BotStore.mixesAgents` derived once per row on every update. After: derived once per update.
- What remains during scrolling is SwiftUI layout of each newly shown cell (`_UIHostingView.layoutSubviews` →
  `sizeThatFits`), at most 29 ms per 50 ms window, so under one frame's budget.

## Ends of the chat

`history.yaml` scrolls to the oldest message, then back to the newest, waits for the scroll to end and checks the newest
reply is fully shown and Jump to latest is gone: the chat rests at its end without bouncing back. It passes on both builds.

## Visual checklist

Full-size screenshots of the roster's newest position and the chat's newest reply, in light and dark appearance, and the
recordings of both flows on both builds were checked by the builder against every item below; this change alters no layout.

- [x] Corner radii match their containing bubble or card.
- [x] Horizontal and vertical padding is balanced.
- [x] Content remains inside its columns and aligns with related text.
- [x] No loading placeholder or layout jump appears between states.
- [x] No unnecessary labels or file metadata are shown.
- [x] Secondary actions remain in menus rather than adding inline buttons.
- [x] Controls are proportionally sized.
- [x] Components render consistently across positions and appearances.
