package com.codync.android

import android.graphics.Bitmap
import android.graphics.Color as AndroidColor
import android.graphics.pdf.PdfRenderer
import android.os.ParcelFileDescriptor
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import com.codync.android.core.FilePreview
import com.codync.android.core.SharedFile
import com.codync.android.design.*
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.io.File
import java.nio.ByteBuffer
import java.nio.charset.CharacterCodingException
import java.nio.charset.CodingErrorAction

/** A shared file being opened in its viewer, and the message that shared it. */
internal data class OpenFile(val entryId: String, val botId: String, val file: SharedFile)

/** A Markdown, text or PDF file a bot shared, read inside the app while `item` is set, with Download in its header. */
@Composable internal fun FileViewerSheet(item: OpenFile?, store: AppStore, dismiss: () -> Unit) {
    CodyncSheet(item, dismiss) { (entryId, botId, file) ->
        Column(Modifier.fillMaxSize().background(Palette.background)) {
            ModalHeader(file.name) {
                Box(Modifier.size(36.dp).plainButton { store.saveFile(entryId, botId, file) }
                    .semantics { contentDescription = "Download ${file.name}" }, contentAlignment = Alignment.Center) {
                    Symbol("arrow.down.to.line", size = 20.dp, tint = Palette.text)
                }
            }
            Box(Modifier.weight(1f).fillMaxWidth()) { FileBody(entryId, file, store) }
        }
    }
}

/** What a viewer shows once the file is read, or why it can't. */
private sealed interface Opened {
    data object Loading : Opened
    data class Text(val text: String) : Opened
    data class Pdf(val document: PdfFile) : Opened
    data class Message(val text: String, val limit: String = "") : Opened
}

@Composable private fun FileBody(entryId: String, file: SharedFile, store: AppStore) {
    val preview = file.preview ?: return
    val opened by produceState<Opened>(Opened.Loading, file.id) {
        value = if (!file.opensInline) Opened.Message(TOO_BIG, limit(preview)) else try {
            val source = store.sharedFile(entryId, file)
            withContext(Dispatchers.IO) {
                if (preview == FilePreview.Pdf) Opened.Pdf(PdfFile(source))
                else utf8(source.readBytes())?.let(Opened::Text) ?: Opened.Message("This file isn't UTF-8 text. Download it to open it in another app.")
            }
        } catch (cancelled: CancellationException) { throw cancelled }
        catch (_: Exception) { Opened.Message("Couldn't open the file.") }
        awaitDispose { (value as? Opened.Pdf)?.document?.close() }
    }
    when (val shown = opened) {
        Opened.Loading -> Centered { Spinner(20.dp) }
        is Opened.Message -> Centered {
            Text(tr(shown.text).replace("{limit}", shown.limit), style = Type.subheadline, color = Palette.secondary, textAlign = TextAlign.Center)
        }
        is Opened.Pdf -> PdfPages(shown.document)
        is Opened.Text -> Box(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(20.dp)) {
            if (preview == FilePreview.Markdown) MarkdownText(shown.text)
            else SelectionContainer { Text(shown.text, style = Type.footnote.monospaced, color = Palette.text) }
        }
    }
}

@Composable private fun Centered(content: @Composable () -> Unit) =
    Box(Modifier.fillMaxSize().padding(32.dp), contentAlignment = Alignment.Center) { content() }

private const val TOO_BIG = "This file is too big to open here (over {limit}). Download it to open it in another app."

/** A kind's limit as people read it: "512 KB", "20 MB". */
private fun limit(preview: FilePreview) =
    if (preview.maxBytes >= 1024 * 1024) "${preview.maxBytes / (1024 * 1024)} MB" else "${preview.maxBytes / 1024} KB"

/** The bytes as UTF-8 text, or null when they aren't. */
private fun utf8(bytes: ByteArray): String? = try {
    Charsets.UTF_8.newDecoder().onMalformedInput(CodingErrorAction.REPORT).onUnmappableCharacter(CodingErrorAction.REPORT)
        .decode(ByteBuffer.wrap(bytes)).toString()
} catch (_: CharacterCodingException) { null }

/** A PDF's pages, each drawn at the viewer's width as it scrolls into view. */
@Composable private fun PdfPages(pdf: PdfFile) = BoxWithConstraints(Modifier.fillMaxSize()) {
    val width = with(LocalDensity.current) { (maxWidth - 24.dp).roundToPx() }
    LazyColumn(contentPadding = PaddingValues(12.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        items(pdf.pageCount) { index ->
            val page by produceState<Bitmap?>(null, pdf, index, width) { value = withContext(Dispatchers.IO) { pdf.render(index, width) } }
            Box(Modifier.fillMaxWidth().aspectRatio(pdf.aspectRatio(index)).background(Color.White)) {
                page?.let { Image(it.asImageBitmap(), contentDescription = "Page ${index + 1}", modifier = Modifier.fillMaxSize()) }
            }
        }
    }
}

/** Android's PDF renderer over one file: it draws one page at a time, so every use holds its lock. */
private class PdfFile(file: File) {
    private val renderer = PdfRenderer(ParcelFileDescriptor.open(file, ParcelFileDescriptor.MODE_READ_ONLY))
    private val ratios = List(renderer.pageCount) { index -> renderer.openPage(index).use { it.width.toFloat() / it.height } }
    private var closed = false
    val pageCount get() = ratios.size
    fun aspectRatio(index: Int) = ratios[index]

    /** Page `index` drawn `width` pixels wide on white, or null once the viewer has closed. */
    @Synchronized fun render(index: Int, width: Int): Bitmap? {
        if (closed) return null
        return renderer.openPage(index).use { page ->
            Bitmap.createBitmap(width, (width / ratios[index]).toInt().coerceAtLeast(1), Bitmap.Config.ARGB_8888).apply {
                eraseColor(AndroidColor.WHITE)
                page.render(this, null, null, PdfRenderer.Page.RENDER_MODE_FOR_DISPLAY)
            }
        }
    }

    @Synchronized fun close() { closed = true; renderer.close() }
}
