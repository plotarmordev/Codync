package com.codync.android

import androidx.compose.ui.unit.IntSize
import org.junit.Assert.*
import org.junit.Test

/** A shared PDF's page bitmap: a page's shape comes from the file, so its pixels must stay bounded. */
class PdfPageSizeTest {
    @Test fun `an ordinary page is drawn at the viewer's width in its own shape`() {
        assertEquals(IntSize(1056, 1494), pageBitmapSize(1056, 1056f / 1494))
    }

    @Test fun `a very tall page keeps its shape but is drawn at most three widths tall`() {
        val size = pageBitmapSize(1080, 1 / 100f)
        assertEquals(3240, size.height)
        assertEquals(32, size.width)
    }

    @Test fun `a very wide page is one pixel tall at least`() {
        assertEquals(IntSize(1080, 1), pageBitmapSize(1080, 100_000f))
    }

    @Test fun `a page on a wide screen stays within the pixel budget`() {
        val size = pageBitmapSize(2560, 1 / 3f)
        assertTrue(size.width.toLong() * size.height <= 4_000_000)
        assertEquals(1f / 3, size.width.toFloat() / size.height, 0.01f)
    }
}
