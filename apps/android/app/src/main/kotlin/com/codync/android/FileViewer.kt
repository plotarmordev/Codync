package com.codync.android

import android.graphics.Bitmap
import android.graphics.Color as AndroidColor
import android.graphics.pdf.PdfRenderer
import android.os.ParcelFileDescriptor
import android.widget.MediaController
import android.widget.VideoView
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
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import com.codync.android.core.FilePreview
import com.codync.android.core.MarkdownBlocks
import com.codync.android.core.SharedFile
import com.codync.android.core.isVideoData
import com.codync.android.design.*
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.io.File
import java.nio.ByteBuffer
import java.nio.charset.CharacterCodingException
import java.nio.charset.CodingErrorAction
import kotlin.math.max
import kotlin.math.min
import kotlin.math.sqrt

/** A shared file being opened in its viewer, and the message that shared it. */
internal data class OpenFile(val entryId: String, val botId: String, val file: SharedFile)

/** A Markdown, text, PDF or video file a bot shared, read inside the app while `item` is set, with Download in its header.
 * `openPdf` reads a PDF's pages (a test's stand-in). */
@Composable internal fun FileViewerSheet(item: OpenFile?, store: AppStore, openPdf: (File) -> PdfDocument = ::PdfFile, dismiss: () -> Unit) {
    CodyncSheet(item, dismiss) { (entryId, botId, file) ->
        Column(Modifier.fillMaxSize().background(Palette.background)) {
            ModalHeader(file.name) {
                Box(Modifier.size(36.dp).plainButton { store.saveFile(entryId, botId, file) }
                    .semantics { contentDescription = "Download ${file.name}" }, contentAlignment = Alignment.Center) {
                    Symbol("arrow.down.to.line", size = 20.dp, tint = Palette.text)
                }
            }
            Box(Modifier.weight(1f).fillMaxWidth()) { FileBody(entryId, file, store, openPdf) }
        }
    }
}

/** What a viewer shows once the file is read, or why it can't. */
private sealed interface Opened {
    data object Loading : Opened
    data class Markdown(val blocks: List<MarkdownBlocks.Block>) : Opened
    data class Text(val lines: List<String>) : Opened
    data class Pdf(val document: PdfDocument) : Opened
    data class Video(val file: File) : Opened
    data class Message(val text: String, val limit: String = "") : Opened
}

@Composable private fun FileBody(entryId: String, file: SharedFile, store: AppStore, openPdf: (File) -> PdfDocument) {
    val preview = file.preview ?: return
    val opened by produceState<Opened>(Opened.Loading, file.id) {
        value = if (!file.opensInline) Opened.Message(TOO_BIG, "${preview.maxBytes / (1024 * 1024)} MB") else try {
            val source = store.sharedFile(entryId, file)
            withContext(Dispatchers.IO) { open(preview, source, openPdf) }
        } catch (cancelled: CancellationException) { throw cancelled }
        catch (_: Exception) { Opened.Message(FAILED) }
        awaitDispose { (value as? Opened.Pdf)?.document?.close() }
    }
    // A page or video that can't be drawn fails the whole file, like one that can't be read.
    var shownFailure by remember(file.id) { mutableStateOf<String?>(null) }
    when (val shown = shownFailure?.let { Opened.Message(it) } ?: opened) {
        Opened.Loading -> Centered { Spinner(20.dp) }
        is Opened.Message -> Centered {
            Text(tr(shown.text).replace("{limit}", shown.limit), style = Type.subheadline, color = Palette.secondary, textAlign = TextAlign.Center)
        }
        is Opened.Pdf -> PdfPages(shown.document) { shownFailure = FAILED }
        is Opened.Video -> VideoPlayer(shown.file) { shownFailure = NOT_VIDEO }
        // Long files stay smooth: blocks and lines are laid out only as they scroll into view.
        is Opened.Markdown -> SelectionContainer {
            LazyColumn(contentPadding = PaddingValues(20.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                items(shown.blocks) { MarkdownBlock(it, Palette.text) }
            }
        }
        is Opened.Text -> SelectionContainer {
            LazyColumn(contentPadding = PaddingValues(20.dp)) {
                items(shown.lines) { Text(it, style = Type.footnote.monospaced, color = Palette.text) }
            }
        }
    }
}

/** The file `source` read for its viewer, or why it can't be. */
private fun open(preview: FilePreview, source: File, openPdf: (File) -> PdfDocument): Opened = when (preview) {
    FilePreview.Pdf -> Opened.Pdf(openPdf(source))
    FilePreview.Video -> if (isVideoData(source.inputStream().use { it.readNBytes(8) })) Opened.Video(source) else Opened.Message(NOT_VIDEO)
    FilePreview.Markdown, FilePreview.Text -> when (val text = utf8(source.readBytes())) {
        null -> Opened.Message("This file isn't UTF-8 text. Download it to open it in another app.")
        else -> if (preview == FilePreview.Markdown) Opened.Markdown(MarkdownBlocks.parse(text)) else Opened.Text(text.lines())
    }
}

/** Android's video view with its play/pause and seek bar, playing `file` as soon as it's ready; `failed` when it can't. */
@Composable private fun VideoPlayer(file: File, failed: () -> Unit) {
    val failedNow by rememberUpdatedState(failed)
    AndroidView(
        factory = { context ->
            VideoView(context).apply {
                setMediaController(MediaController(context).also { it.setAnchorView(this) })
                setOnPreparedListener { start() }
                setOnErrorListener { _, _, _ -> failedNow(); true }
                setVideoPath(file.path)
            }
        },
        onRelease = { it.stopPlayback() },
        modifier = Modifier.fillMaxSize().background(Color.Black),
    )
}

@Composable private fun Centered(content: @Composable () -> Unit) =
    Box(Modifier.fillMaxSize().padding(32.dp), contentAlignment = Alignment.Center) { content() }

private const val TOO_BIG = "This file is too big to open here (over {limit}). Download it to open it in another app."
private const val FAILED = "Couldn't open the file."
private const val NOT_VIDEO = "This video can't play here. Download it to open it in another app."

/** The bytes as UTF-8 text, or null when they aren't. */
private fun utf8(bytes: ByteArray): String? = try {
    Charsets.UTF_8.newDecoder().onMalformedInput(CodingErrorAction.REPORT).onUnmappableCharacter(CodingErrorAction.REPORT)
        .decode(ByteBuffer.wrap(bytes)).toString()
} catch (_: CharacterCodingException) { null }

/** A PDF's pages, each drawn at the viewer's width as it scrolls into view; `failed` when one can't be drawn. */
@Composable private fun PdfPages(pdf: PdfDocument, failed: () -> Unit) = BoxWithConstraints(Modifier.fillMaxSize()) {
    val width = with(LocalDensity.current) { (maxWidth - 24.dp).roundToPx() }
    LazyColumn(contentPadding = PaddingValues(12.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        items(pdf.pageCount) { index ->
            val page by produceState<Bitmap?>(null, pdf, index, width) {
                value = try { withContext(Dispatchers.IO) { pdf.render(index, width) } }
                catch (cancelled: CancellationException) { throw cancelled }
                catch (_: Exception) { failed(); null }
            }
            Box(Modifier.fillMaxWidth().aspectRatio(max(pdf.aspectRatio(index), 1 / MAX_PAGE_HEIGHTS)).background(Color.White)) {
                page?.let { Image(it.asImageBitmap(), contentDescription = "Page ${index + 1}", modifier = Modifier.fillMaxSize()) }
            }
        }
    }
}

/** The PDF sets its pages' shape, however extreme: a page shows at most this many widths tall… */
private const val MAX_PAGE_HEIGHTS = 3f
/** …and is drawn in at most this many pixels (16 MiB as ARGB_8888). */
private const val MAX_PAGE_PIXELS = 4_000_000f

/** The bitmap for a page `ratio` (width / height) shown `width` pixels wide: its own shape, shrunk to fit
 * [MAX_PAGE_HEIGHTS] widths and [MAX_PAGE_PIXELS]. */
internal fun pageBitmapSize(width: Int, ratio: Float): IntSize {
    val height = min(width / ratio, width * MAX_PAGE_HEIGHTS)
    val scale = min(1f, sqrt(MAX_PAGE_PIXELS / (height * ratio * height)))
    return IntSize((height * ratio * scale).toInt().coerceAtLeast(1), (height * scale).toInt().coerceAtLeast(1))
}

/** A PDF's pages for its viewer. */
internal interface PdfDocument : AutoCloseable {
    val pageCount: Int
    /** Page `index`'s width / height. */
    fun aspectRatio(index: Int): Float
    /** Page `index` drawn for a viewer `width` pixels wide on white, or null once the viewer has closed. */
    fun render(index: Int, width: Int): Bitmap?
}

/** Android's PDF renderer over one file: it draws one page at a time, so every use holds its lock. */
private class PdfFile(file: File) : PdfDocument {
    private val renderer = PdfRenderer(ParcelFileDescriptor.open(file, ParcelFileDescriptor.MODE_READ_ONLY))
    private val ratios = List(renderer.pageCount) { index -> renderer.openPage(index).use { it.width.toFloat() / it.height } }
    private var closed = false
    override val pageCount get() = ratios.size
    override fun aspectRatio(index: Int) = ratios[index]

    @Synchronized override fun render(index: Int, width: Int): Bitmap? {
        if (closed) return null
        val size = pageBitmapSize(width, ratios[index])
        return renderer.openPage(index).use { page ->
            Bitmap.createBitmap(size.width, size.height, Bitmap.Config.ARGB_8888).apply {
                eraseColor(AndroidColor.WHITE)
                page.render(this, null, null, PdfRenderer.Page.RENDER_MODE_FOR_DISPLAY)
            }
        }
    }

    @Synchronized override fun close() { closed = true; renderer.close() }
}
