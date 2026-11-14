<!--
Google Play release notes for the Android app. Write it in the change that bumps the version:
a `## <version>` section with one `### <locale>` per Play language (zh-Hant, published as zh-TW;
en-US), listing what Android users notice, each under 500 characters (Play's limit;
tools/play-submit.py cuts longer notes at the last whole line). tools/play-submit.py reads the
released version's section; a version without one gets the PRs' "What's New: Android" bullets,
else a generic "Bug fixes and improvements" line. Delete old sections.
-->

## 2.47.0
### zh-Hant
- 從任何 App 的分享選單選「Codync」，就能把文字、連結、照片或檔案傳給 Bot，還能附上一段訊息。上次分享的對象會排在「最近」並預先選好，原本輸入框裡的草稿不會被動到。
- 更新提示會清楚指出需要更新這個 App，還是電腦上的 Codync；雲端電腦更新時也會顯示進度說明。
- 在語音訊息載入時離開聊天，該語音不會在離開後才開始播放。
- 選擇卡片改用編號選項；最後一列「其他答案」可以直接打字回答，bot 會帶著你的話繼續。
- Claude Code 與 Codex 可以在同一台電腦登入多個賬戶：在智慧體的設定頁「其他賬戶」新增，再到 bot 設定的「智慧體」選擇它要用的賬戶，兩個 bot 就能同時用不同賬戶工作。
- 手機重新連上電腦後（包括電腦上的 Codync 更新之後），只補上錯過的訊息，不再重新載入每個聊天。
- 語音訊息一出現就顯示播放鍵、進度條和長度（例如 0:00 / 0:09）；單獨傳來的音檔也用同一個播放器，下載移到訊息的選單裡。無法播放時，錯誤訊息直接取代進度條。

### en-US
- Share text, links, photos or files to a bot from any app: choose Codync in the share sheet and add a message if you like. The bots you shared to last are under Recent, with the latest one already picked.
- Update notices clearly identify whether this app or Codync on your computer needs updating, and explain when a cloud computer is updating itself.
- Leaving a chat while a voice note is loading no longer lets it start playing afterward.
- Choice cards show numbered options, and the last row, Something else, takes your own answer; the bot continues with your words.
- Claude Code and Codex can be signed in to more than one account on a computer: add one under Other accounts in the agent's setup, then pick a bot's account under Agent in its settings, so two bots can work at once on different accounts.
- When your phone reconnects to a computer, including after Codync on it updates, it fetches only what it missed instead of reloading every chat.
- A voice note shows its play button, progress bar and length (like 0:00 / 0:09) from the start, and a sound file sent on its own uses the same player; Download is in the message's menu. When a note can't play, the error takes the bar's place.
