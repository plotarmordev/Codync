<!--
Google Play release notes for the Android app. Write it in the change that bumps the version:
a `## <version>` section with one `### <locale>` per Play language (zh-Hant, published as zh-TW;
en-US), listing what Android users notice, each under 500 characters (Play's limit;
tools/play-submit.py cuts longer notes at the last whole line). tools/play-submit.py reads the
released version's section; a version without one gets the PRs' "What's New: Android" bullets,
else a generic "Bug fixes and improvements" line. Delete old sections.
-->

## 2.38.1
### zh-Hant
- 語音訊息載入時會顯示轉圈，播放時顯示已播放時間、總長度與進度條。
- 無法播放的音訊會在卡片上顯示「無法播放」，點一下即可重試，不再跳出對話框。
- 檔案卡片的按鈕更好點，檔案大小與 iPhone 和電腦版一致；使用 TalkBack 時每張卡片只需滑一次。
- Bot 用語音回覆時，播放器會直接出現在那則訊息的泡泡頂端，文字在下方。
- 聽寫多了「這台電腦」引擎（英文）：由你的 Mac 用自己的語音模型轉成文字，不需要金鑰，聲音不會離開你的電腦。
- 機器人分享的 Markdown、文字、程式碼、CSV、JSON 與 PDF 檔案，點一下就能在 App 內開啟閱讀；影片（MP4、MOV、M4V、WebM）也能直接在 App 內播放，下載按鈕一樣都在。

### en-US
- Voice notes show a spinner while they load, then the time played, their length and a progress bar.
- A sound that can't play says so on its card; tap to try again, with no dialog.
- File card buttons are easier to tap, file sizes match the iPhone and desktop apps, and TalkBack reads each card as one item.
- When a bot speaks its reply, the voice note plays at the top of that message's bubble, with the text under it.
- Dictation has a new engine for English, This computer: your Mac transcribes what you say with its own speech model, with no key, and the audio never leaves your computer.
- Markdown, text, code, CSV, JSON and PDF files a bot shares open right in the app with a tap, and shared videos (MP4, MOV, M4V, WebM) play there, with Download still at hand.
