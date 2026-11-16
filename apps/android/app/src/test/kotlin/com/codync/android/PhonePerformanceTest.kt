package com.codync.android

import org.junit.Assert.assertEquals
import org.junit.Test

class PhonePerformanceTest {
    @Test fun `open timing uses host diagnostic wire shape`() {
        assertEquals("""{"type":"open_timing","platform":"android","phase":"catch_up","open":"foreground","ms":1250,"chats":8,"messages":31}""",
            PhonePerformance(OpenPhase.CatchUp, OpenKind.Foreground, 1250, chats = 8, messages = 31).body().toString())
    }
}
