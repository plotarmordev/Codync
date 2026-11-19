package com.codync.android

import android.graphics.Bitmap
import androidx.activity.compose.setContent
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import com.codync.android.core.SharedFile
import kotlinx.coroutines.CancellationException
import org.junit.After
import org.junit.Rule
import org.junit.Test
import java.io.File
import java.util.UUID

/** A PDF whose pages can't be drawn once it has opened: the viewer says so and still offers Download. */
class FileViewerTest {
    @get:Rule val compose = createAndroidComposeRule<MainActivity>()
    private val store = previewStore()
    private val file = SharedFile(UUID.randomUUID().toString(), "report.pdf", 4, "0".repeat(64))

    /** One page that throws `error` when it's drawn. */
    private class FailingPages(private val error: Exception) : PdfDocument {
        @Volatile var drawn = false
        override val pageCount = 1
        override fun aspectRatio(index: Int) = 0.75f
        override fun render(index: Int, width: Int): Bitmap? { drawn = true; throw error }
        override fun close() {}
    }

    /** Opens the viewer on [file], already in the cache so nothing is fetched, with [pages] as its PDF. */
    private fun open(pages: FailingPages) {
        store.attachments.cached(file.id).writeBytes(ByteArray(file.size.toInt()))
        compose.runOnUiThread { compose.activity.setContent { Hosted { FileViewerSheet(OpenFile("entry", "bot", file), store, { pages }) {} } } }
        compose.waitUntil(10_000) { pages.drawn }
        compose.waitForIdle()
    }

    @After fun cleanup() { store.attachments.cached(file.id).delete() }

    @Test fun aPageThatFailsToDrawShowsTheErrorAndKeepsDownload() {
        open(FailingPages(IllegalStateException("page")))
        compose.onNodeWithText("Couldn't open the file.").assertIsDisplayed()
        compose.onNodeWithContentDescription("Download report.pdf").assertIsDisplayed()
    }

    @Test fun aCancelledPageIsNotShownAsAFailure() {
        open(FailingPages(CancellationException("scrolled away")))
        compose.onNodeWithText("Couldn't open the file.").assertDoesNotExist()
        compose.onNodeWithContentDescription("Download report.pdf").assertIsDisplayed()
    }
}
