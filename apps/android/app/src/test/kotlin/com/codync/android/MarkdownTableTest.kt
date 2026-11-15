package com.codync.android

import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** iOS `tableColumnsFitOrShareTheBubble`. */
class MarkdownTableTest {
    @Test fun `columns keep their widest cell when they fit`() {
        assertArrayEquals(intArrayOf(40, 60), tableColumnWidths(intArrayOf(40, 60), 200))
    }

    @Test fun `columns share the bubble when they do not fit`() {
        val widths = tableColumnWidths(intArrayOf(40, 600), 200)
        assertEquals(40, widths[0])
        assertTrue(widths[1] < 600)
        assertTrue(widths.sum() <= 200)
    }

    @Test fun `a long token gets less than its width so it wraps`() {
        val widths = tableColumnWidths(intArrayOf(30, 900), 120)
        assertTrue(widths[1] < 900)
        assertTrue(widths.sum() <= 120)
    }
}
