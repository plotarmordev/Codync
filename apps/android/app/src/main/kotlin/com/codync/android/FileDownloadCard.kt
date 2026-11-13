package com.codync.android

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.sizeIn
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.semantics.CustomAccessibilityAction
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.customActions
import androidx.compose.ui.semantics.onClick
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.codync.android.core.Entry
import com.codync.android.core.SharedFile
import com.codync.android.design.*
import com.kyant.shapes.RoundedRectangle
import kotlin.math.roundToInt

/** A file a bot shared that stays a file (a `playsInChat` sound gets the voice note bubble instead), as iOS
 *  `FileDownloadCard`: an image shows itself (tap for full size), a Markdown, text or PDF file opens on tap, then its
 *  name and size, and a button to download (or cancel). One accessibility element: `<name>, <caption>` plus the
 *  actions. Holding it opens its message's `menu`; an image's copies the picture or its name. */
@Composable internal fun FileDownloadCard(entry: Entry, file: SharedFile, menu: MessageMenu, state: AppState, store: AppStore) {
    val context = LocalContext.current
    val focus = LocalFocusManager.current
    var viewing by remember { mutableStateOf(false) }
    var reading by remember { mutableStateOf<OpenFile?>(null) }
    val translate = translator()
    val download = state.fileDownloads[file.id]
    val busy = download is FileDownloadState.Downloading
    val caption = detail(file, download, tr = translate)
    val save = { if (busy) store.cancelFile(file.id) else store.saveFile(entry.id, entry.botId, file) }
    val open = if (file.preview != null) ({ reading = OpenFile(entry.id, entry.botId, file) }) else null
    val view = if (file.showsAsImage) ({ viewing = true }) else open
    Column(Modifier.fillMaxWidth().contextActions(reactions = menu.reactions, onClick = { focus.clearFocus() }) {
        menu.items(if (file.showsAsImage) imageCopyItems(store, file.imageSource(entry.id, store)) else listOf(textCopyItem(context, entry.text)))
    }.clip(RoundedRectangle(16.dp)).background(Palette.bubbleAgent)
        .clearAndSetSemantics {
            contentDescription = "${file.name}, $caption"
            customActions = listOfNotNull(
                CustomAccessibilityAction(translate(if (busy) "Cancel download" else "Download")) { save(); true },
                view?.let { open -> CustomAccessibilityAction(translate("View")) { open(); true } })
            onClick(label = translate(if (view != null) "View" else if (busy) "Cancel download" else "Download")) { (view ?: save)(); true }
        }.padding(14.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        if (file.showsAsImage) SharedImage(file, entry.id, 720, state, store, Modifier.sizeIn(maxHeight = 280.dp)
            .clip(RoundedRectangle(10.dp)).plainButton { viewing = true }.semantics { contentDescription = file.name })
        FileRow(entry, file, state, store, caption, open)
    }

    SharedImageSheet(file.takeIf { viewing }, entry, menu, state, store) { viewing = false }
    FileViewerSheet(reading, store) { reading = null }
}

/** Its icon, name and caption, which `open` a readable file on tap, and the download button. */
@Composable private fun FileRow(entry: Entry, file: SharedFile, state: AppState, store: AppStore, caption: String, open: (() -> Unit)?) {
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        Row(Modifier.weight(1f).then(if (open != null) Modifier.plainButton(action = open) else Modifier),
            verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            Symbol(AttachmentIcon.symbol(file.name), size = 24.dp, tint = Palette.secondary)
            Column(verticalArrangement = Arrangement.spacedBy(3.dp)) {
                Text(file.name, style = Type.subheadline.semibold, color = Palette.text, maxLines = 2, overflow = TextOverflow.Ellipsis)
                Text(caption, style = Type.caption2, color = Palette.secondary)
            }
        }
        DownloadButton(entry, file, state, store)
    }
}

/** Seconds as `m:ss` (`h:mm:ss` past an hour). */
internal fun clock(seconds: Double): String {
    val total = seconds.roundToInt()
    return if (total >= 3600) "%d:%02d:%02d".format(total / 3600, total / 60 % 60, total % 60) else "%d:%02d".format(total / 60, total % 60)
}

/** A shared file's download button, trailing on a file card and in the file's
 *  viewer: downloads (again after a failure or a save), or cancels the download under way. In the image viewer
 *  (`inViewer`) its end is reported. */
@Composable internal fun DownloadButton(entry: Entry, file: SharedFile, state: AppState, store: AppStore, inViewer: Boolean = false) {
    val busy = state.fileDownloads[file.id] is FileDownloadState.Downloading
    Box(Modifier.size(36.dp).plainButton { if (busy) store.cancelFile(file.id) else store.saveFile(entry.id, entry.botId, file, if (inViewer) ::reportViewerDownload else null) }
        .semantics { contentDescription = (if (busy) "Cancel download " else "Download ") + file.name }, contentAlignment = Alignment.Center) {
        Symbol(if (busy) "xmark" else "arrow.down.to.line", size = 20.dp, tint = Palette.text)
    }
}

/** How a download from the image viewer ended. */
private fun reportViewerDownload(error: ImageError?) = Analytics.capture(AnalyticsEvent.ImagePreviewAction(ImageAction.Download, ImageFrom.Viewer, error))

/** A shared file's caption: how its download goes, then its size. */
internal fun detail(file: SharedFile, download: FileDownloadState?, tr: (String) -> String = { it }): String {
    val size = formatBytes(file.size)
    return when (download) {
        is FileDownloadState.Downloading -> tr("{received} of {size}").replace("{received}", formatBytes(download.received)).replace("{size}", size)
        is FileDownloadState.Failed -> "${download.message} ${tr("Tap to retry.")}"
        FileDownloadState.Saved -> "$size · ${tr("Downloaded")}"
        null -> size
    }
}

/** A voice note's caption, never a byte count: a player failure, then a failed download, then the sound's clock;
 *  empty (the bar alone) while its length is unknown. */
internal fun voiceNoteDetail(download: FileDownloadState?, audio: SharedAudio, tr: (String) -> String = { it }): String = when {
    audio.phase == AudioPhase.Failed -> tr("Couldn't play. Tap to retry.")
    download is FileDownloadState.Failed -> "${download.message} ${tr("Tap to retry.")}"
    audio.duration > 0.0 -> "${clock(audio.elapsed)} / ${clock(audio.duration)}"
    else -> ""
}
