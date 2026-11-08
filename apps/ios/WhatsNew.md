<!--
App Store "What's New" for the iPhone app. Write it in the change that bumps the version:
a `## <version>` section with one `### <locale>` per App Store language (zh-Hant, en-US),
listing what iPhone users notice, without emoji (the App Store rejects them; asc-submit.py drops
any left). tools/asc-submit.py reads the released version's section;
a version without one gets a generic "Bug fixes and improvements" line. Delete old sections.
-->

## 2.42.0
### zh-Hant
- 聽寫可以選你的電腦（以電腦名稱列出）：由你自己架設的語音模型轉成文字（在電腦版 Codync 的語音設定中連結），不需要金鑰，聲音不會離開你的電腦。

### en-US
- Dictation can use your computer, listed under its name: a speech model you run yourself transcribes what you say (link it in Voice chat settings in Codync on your computer), with no key, and the audio never leaves your computers.

## 2.38.1
### zh-Hant
- 語音訊息載入時會顯示轉圈，播放時顯示已播放時間、總長度與進度條。
- 無法播放的音訊會在卡片上顯示「無法播放」，點一下即可重試，不再跳出對話框。
- 檔案卡片的按鈕更好點，Artifact 卡片與檔案卡片同寬；使用 VoiceOver 時每張卡片只需滑一次。
- Bot 用語音回覆時，播放器會直接出現在那則訊息的泡泡頂端，文字在下方；播放鍵、較粗的進度條與時間排成一列，點或拖動進度條即可跳到想聽的地方。
- 聽寫多了「這台電腦」引擎（英文）：由你的 Mac 用自己的語音模型轉成文字，不需要金鑰，聲音不會離開你的電腦。

### en-US
- Voice notes show a spinner while they load, then the time played, their length and a progress bar.
- A sound that can't play says so on its card; tap to try again, with no dialog.
- File card buttons are easier to tap, artifact cards line up with file cards, and VoiceOver reads each card as one item.
- When a bot speaks its reply, the voice note plays at the top of that message's bubble, with the text under it: play button, a thicker progress bar and the time on one row; tap or drag the bar to jump to any point.
- Dictation has a new engine for English, This computer: your Mac transcribes what you say with its own speech model, with no key, and the audio never leaves your computer.
