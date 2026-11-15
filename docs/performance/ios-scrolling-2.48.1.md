# iPhone scrolling measurements (2.48.1)

Simulator: iPhone 17, iOS 27.2. Load: 60 bots; 160 turns / 320 messages in the long-chat flow; a separate 300-entry Markdown streaming reply. These are simulator measurements, not physical-iPhone results.

## Before / after

| Scenario | Before | After |
| --- | ---: | ---: |
| Streaming update, average | 43.75 ms | 24.71 ms |
| Streaming update, p95 | 51.01 ms | 29.71 ms |
| Roster update, average | 34.12 ms | 12.42 ms |
| Roster update, p95 | 38.46 ms | 13.91 ms |
| Long-chat scroll up, p95 | 5.37 ms | 5.84 ms |
| Long-chat scroll down, p95 | 2.39 ms | 2.24 ms |

The scroll-up change is 0.47 ms and remains below one 60 Hz frame. The update probes show the reduced main-thread work when rows change; the screen recordings are the visual check for continuous scrolling and clean edge settling.

## Profiler results

Time Profiler traces: `probe-before.trace`, `probe-after.trace`, `streaming-time-profile.trace`, `scroll-time-profile.trace`, and `bot-list-time-profile.trace`.

The measured chat costs were repeated `ChatItem.build` / `BotExchangeGroup.collapse`, `MarkdownText.inline`, `AttributedString` mutation, and `NSAttributedString` / UIKit conversion during row rendering. The roster cost was repeated `BotStore.mixesAgents` derivation for every row. The change caches unchanged chat items and rows, and derives roster filtering and mixed-agent state once per update instead of once per row.

## Recorded flows

- `bot-list-scroll.mp4`: 60-bot roster, scrolling to older bots and back to the newest bot.
- `long-chat-scroll.mp4`: 320-message conversation, scrolling from the latest reply to message 1.

The repeatable roster regression is `apps/maestro/roster-scroll.yaml`; the long-chat regression is `apps/maestro/history.yaml`.

## Full-size visual checklist

Screenshots cover the long chat's oldest message in light appearance, the roster's newest position in light and dark appearances, and its oldest, intermediate, and settled positions in light appearance. The recordings cover both ends of the chat and a round trip through the roster. Builder and reviewer checked every item below; no new defect was found. Floating glass chrome over scrolled content is the existing native design, and identical roster metadata is synthetic fixture data.

- [x] Corner radii match their containing bubble or card.
- [x] Horizontal and vertical padding is balanced.
- [x] Content remains inside its columns and aligns with related text.
- [x] No loading placeholder or layout jump appears between states.
- [x] No unnecessary labels or file metadata are shown.
- [x] Secondary actions remain in menus rather than adding inline buttons.
- [x] Controls are proportionally sized.
- [x] Components render consistently across positions and appearances.

Physical-iPhone validation remains a TestFlight follow-up.
