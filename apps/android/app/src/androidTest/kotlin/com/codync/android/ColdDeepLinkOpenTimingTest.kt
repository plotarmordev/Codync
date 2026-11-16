package com.codync.android

import androidx.compose.foundation.layout.Box
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.junit4.createComposeRule
import com.codync.android.core.*
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test

class ColdDeepLinkOpenTimingTest {
    @get:Rule val compose = createComposeRule()

    @Test fun coldDeepLinkRecordsOpenChatOnlyAfterTheLaunchSplashLeaves() {
        val store = previewStore()
        val bot = Bot("deep-link-bot", "Scout")
        val state = AppState(previewComputer, loading = false, selectedBot = bot.id, bots = listOf(bot),
            connection = LinkState.Ready(HostRoute.Direct), actualConnection = LinkState.Ready(HostRoute.Direct),
            historyEnd = mapOf(bot.id to HistoryEnd.Complete), opensSeen = 1, catchUpsCompleted = 1,
            entries = listOf(Entry("deep-link-message", 1, bot.id, rev = 1, kind = "agent",
                data = buildJsonObject { put("text", "Ready."); put("final", true) })))
        val events = mutableListOf<AnalyticsEvent.OpenTiming>()
        val timing = OpenTiming { (it as? AnalyticsEvent.OpenTiming)?.let(events::add) }
        compose.runOnUiThread {
            store.openTiming = timing
            timing.beginCold()
            timing.record(OpenPhase.CatchUp, previewComputer.id, chats = 0, messages = 0)
        }
        val splash = hasContentDescription("Connecting")
        compose.setContent {
            Hosted {
                var launching by remember { mutableStateOf(true) }
                Box {
                    ChatScreen(state, store, editGroup = {}, launching = launching)
                    LaunchLayer(loading = false, caughtUp = true) { launching = false }
                }
            }
        }
        compose.mainClock.autoAdvance = false
        compose.onNode(splash).assertExists()
        fun openChats() = events.count { it.phase == OpenPhase.OpenChat }
        assertEquals(0, openChats())

        // The splash's delay() runs on the test scheduler; only the System.currentTimeMillis() subtraction that
        // shortens its 0.7 s hold is wall clock. Well inside the hold nothing may have been recorded.
        compose.mainClock.advanceTimeBy(400)
        compose.waitForIdle()
        assertEquals(0, openChats())

        // Hold, then the 0.3 s fade-out: covered the whole way, so `open_chat` can only exist once the row is gone.
        // `System.currentTimeMillis()` subtracts a few wall milliseconds from the hold; 100 frames is 1.6 s of slack.
        repeat(100) {
            compose.mainClock.advanceTimeByFrame()
            compose.waitForIdle()
            if (compose.onAllNodes(splash).fetchSemanticsNodes().isEmpty()) return@repeat
            assertEquals(0, openChats())
        }
        compose.onNode(splash).assertDoesNotExist()
        compose.waitUntil(5_000) { openChats() == 1 }
        assertEquals(1, openChats())
    }
}
