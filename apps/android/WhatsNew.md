<!--
Google Play release notes for the Android app. Write it in the change that bumps the version:
a `## <version>` section with one `### <locale>` per Play language (zh-Hant, published as zh-TW;
en-US), listing what Android users notice, each under 500 characters (Play's limit;
tools/play-submit.py cuts longer notes at the last whole line). tools/play-submit.py reads the
released version's section; a version without one gets the PRs' "What's New: Android" bullets,
else a generic "Bug fixes and improvements" line. Delete old sections.
-->

## 2.49.0
### zh-Hant
- Bot 把比較久的工作放到背景做，聊天照常回你。
- 例行任務和其他 Bot 的請求也在背景進行，不再占住聊天。
- Bot 列表的「工作」按鈕列出每個 Bot 的背景工作。
- 背景工作要核准、要問你問題或傳檔案時，會出現在聊天裡。
- 點輸入框上方的膠囊開啟 Bot 的「詳情」：Threads（正在跑的）和 Event Box（等待中的：先做、插隊、丟掉）。
- 「同時幾個 agent」在 Bot 的「設定」。
- 「每個 agent 一個 worktree」：同時在同一個 repository 工作的 agent 各有自己的分支。
- 改用備援時，智慧體卡片會說明原因。
- 輸入 `/` 時會列出 Bot 所用智慧體自己的指令。
- 語音訊息一出現就顯示播放鍵、進度條和長度，進度條兩側間距相同，聲音先在背景載入，按下就播放（行動數據時只預先載入 2 MB 以下）；下載移到訊息的選單裡；載入時離開聊天，它不會在離開後才開始播放。
- 重新連上電腦後只補上錯過的訊息。
- 訊息裡的表格放不進泡泡時，過長的文字會在格子裡換行，不再被截斷。

### en-US
- Bots do long work in the background and keep answering.
- Routines and other bots' requests run there too.
- The Work button in the bot list shows every bot's background work.
- Background work can ask, or send a file, in the chat.
- Details: Threads (running), Event box (waiting: First, Cut in, Drop).
- Settings: Agents at once; Worktree per agent gives each its own branch.
- On a fallback, the Agent card says why.
- `/` also lists the agent's own commands.
- Voice notes show a player at once, load ahead so they play on tap (under 2 MB on mobile data), and stop when you leave.
- Reconnects fetch only missed messages.
- A table that doesn't fit its message bubble wraps the long text inside its cells instead of cutting it off.
