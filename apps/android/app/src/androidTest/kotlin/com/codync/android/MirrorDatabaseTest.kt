package com.codync.android

import androidx.test.platform.app.InstrumentationRegistry
import android.database.sqlite.SQLiteDatabase
import android.content.ContentValues
import com.codync.android.core.*
import java.io.File
import java.util.UUID
import kotlinx.serialization.json.*
import kotlinx.serialization.encodeToString
import org.junit.After
import org.junit.Assert.*
import org.junit.Test

class MirrorDatabaseTest {
    private val context = InstrumentationRegistry.getInstrumentation().targetContext
    private val directory = File(context.noBackupFilesDir, "mirror-test-" + UUID.randomUUID()).apply { mkdirs() }
    private val computer = "AAAAAAAAAAAAAAAAAAAAAA"
    private var mirror = MirrorDatabase(context, directory, computer)
    private fun entry(rev: Long, text: String = "new", nonce: String? = null): Entry = Entry("message", 1, "bot", rev = rev, kind = "user",
        data = buildJsonObject { put("text", text); nonce?.let { put("clientNonce", it) } })

    @After fun cleanup() { mirror.close(); directory.deleteRecursively() }

    @Test fun iconCacheTracksNewerRevisionsAndResetsWithTheHostDatabase() {
        mirror.setHost("old", MirrorDatabase.FORMAT)
        val first = CustomIcon("first")
        val latest = CustomIcon("latest", true)
        mirror.apply(MirrorEvent.Icons(mapOf("bot:b" to first), 1))
        val cached = mirror.icons()
        assertSame(cached, mirror.icons())
        MirrorDatabase(context, directory, computer).use { other ->
            other.apply(MirrorEvent.Icons(mapOf("bot:b" to latest), 2))
        }
        assertEquals(latest, mirror.icons()["bot:b"])
        mirror.apply(MirrorEvent.Icons(mapOf("bot:b" to first), 1))
        assertEquals(latest, mirror.icons()["bot:b"])
        mirror.apply(MirrorEvent.Icons(emptyMap(), 3))
        assertTrue(mirror.icons().isEmpty())
        mirror.setHost("new", MirrorDatabase.FORMAT)
        mirror.apply(MirrorEvent.Icons(mapOf("bot:b" to first), 0))
        assertEquals(first, mirror.icons()["bot:b"])
    }

    @Test fun reconnectAcceptsSameHostRollbackAboveBotCursorAndRejectsOldSaveReplies() {
        mirror.setHost("same-host", MirrorDatabase.FORMAT)
        mirror.apply(MirrorEvent.BotChanged(Bot("b", "Bot", rev = 50)))
        val old = CustomIcon("old")
        mirror.apply(MirrorEvent.Icons(mapOf("bot:b" to old), 100))
        val oldEpoch = mirror.iconEpoch()
        val currentEpoch = mirror.beginIconConnection()
        assertEquals(old, mirror.icons()["bot:b"]) // Keep the offline picture until catch-up arrives.
        mirror.apply(MirrorEvent.Hello("same-host", 90, JsonObject(emptyMap()), null))
        val restored = CustomIcon("restored")
        mirror.apply(MirrorEvent.Icons(mapOf("bot:b" to restored), 90))
        assertEquals(50, mirror.revision())
        assertEquals(restored, mirror.icons()["bot:b"])
        assertFalse(mirror.applyIconReply(MirrorEvent.Icons(mapOf("bot:b" to old), 110), oldEpoch))
        assertEquals(restored, mirror.icons()["bot:b"])
        assertTrue(mirror.applyIconReply(MirrorEvent.Icons(emptyMap(), 91), currentEpoch))
        assertTrue(mirror.icons().isEmpty())
    }

    @Test fun rpcRepliesAndHelloCannotCheckpointPastUnseenEvents() {
        mirror.apply(MirrorEvent.Hello("database", 100, JsonObject(emptyMap()), null))
        mirror.receive(listOf(entry(90)))
        assertEquals(0, mirror.snapshot().rev)
        mirror.apply(MirrorEvent.EntryChanged(entry(2, "older")))
        assertEquals(2, mirror.snapshot().rev)
        assertEquals("new", mirror.snapshot().entries.single().text)
    }

    @Test fun anInitialRosterCannotSkipEarlierTranscriptEventsAndBatchesCommitTogether() {
        mirror.receive(listOf(Bot("bot", "Current roster", rev = 500)))
        assertEquals(0, mirror.snapshot().rev)
        mirror.save(PendingSend("nonce", "bot", text = "message", createdAt = 1))
        mirror.apply(listOf(MirrorEvent.BotChanged(Bot("bot", "Older roster", rev = 2)),
            MirrorEvent.EntryChanged(entry(3, nonce = "nonce"))))
        mirror.close()
        mirror = MirrorDatabase(context, directory, computer)
        val snapshot = mirror.snapshot()
        assertEquals(3, snapshot.rev)
        assertEquals("Current roster", snapshot.bots.single().name)
        assertEquals("nonce", snapshot.entries.single().clientNonce)
        assertTrue(snapshot.pending.isEmpty())
    }

    @Test fun cursorAndSendReconciliationSurviveReopeningTogether() {
        mirror.save(PendingSend("nonce", "bot", text = "message", createdAt = 1))
        mirror.apply(MirrorEvent.EntryChanged(entry(7, nonce = "nonce")))
        mirror.close()
        mirror = MirrorDatabase(context, directory, computer)
        assertEquals(7, mirror.snapshot().rev)
        assertTrue(mirror.snapshot().pending.isEmpty())
        assertEquals("nonce", mirror.snapshot().entries.single().clientNonce)
    }

    @Test fun tombstonesCannotBeResurrectedByLateRepliesOrHistory() {
        mirror.apply(MirrorEvent.BotChanged(Bot("bot", "Bot", rev = 2)))
        mirror.apply(MirrorEvent.EntryChanged(entry(3)))
        mirror.apply(MirrorEvent.BotChanged(Bot("bot", rev = 4, deleted = true)))
        mirror.receive(Bot("bot", "Old", rev = 2))
        mirror.receive(listOf(entry(3)))
        assertTrue(mirror.snapshot().bots.isEmpty())
        assertTrue(mirror.snapshot().entries.isEmpty())
        assertEquals(4, mirror.snapshot().rev)
    }

    @Test fun incompatibleCacheAndDatabaseReplacementRewindSafely() {
        assertFalse(mirror.setHost("old", 1))
        mirror.apply(MirrorEvent.EntryChanged(entry(9)))
        mirror.save(PendingSend("unsent", "bot", text = "message", createdAt = 1))
        mirror.finishHistory("bot")
        assertFalse(mirror.setHost("old", 1))
        assertEquals(1, mirror.snapshot().entries.size)
        // Another mirror format: the catch-up from 0 brings only what the host still has.
        assertTrue(mirror.setHost("old", 2))
        assertEquals(0, mirror.snapshot().rev)
        assertFalse(mirror.historyComplete("bot"))
        assertEquals("unsent", mirror.snapshot().pending.single().nonce)
        mirror.apply(MirrorEvent.EntryChanged(Entry("kept", 2, "bot", rev = 10, kind = "user", data = buildJsonObject { put("text", "kept") })))
        assertEquals(listOf("kept"), mirror.snapshot().entries.map { it.id })
        assertTrue(mirror.setHost("new", 2))
        assertEquals(0, mirror.snapshot().rev)
        assertTrue(mirror.snapshot().entries.isEmpty())
    }

    @Test fun aNewVersionReadsTheBotsAgainAndKeepsTheConversation() {
        mirror.setHost("host", MirrorDatabase.FORMAT, "8")
        mirror.apply(MirrorEvent.EntryChanged(entry(9)))
        assertTrue(mirror.rosterStale("2.4/2.4"))
        mirror.receiveRoster(listOf(Bot("bot", "Bot", rev = 2)), "2.4/2.4")
        assertFalse(mirror.rosterStale("2.4/2.4"))
        // The cursor stays: a catch-up from the start onto the rows held could leave a gap.
        assertEquals(9, mirror.revision())
        assertTrue(mirror.rosterStale("2.5/2.4"))
        assertFalse(mirror.setHost("host", MirrorDatabase.FORMAT, "8"))
        assertEquals(1, mirror.snapshot().entries.size)
    }

    @Test fun anotherStoreSchemaDropsTheConversationAndAnUnknownOneKeepsIt() {
        // As 2.44.1 left it: a stamp of the app and host versions, which says nothing about the schema.
        mirror.writableDatabase.insertOrThrow("meta", null, ContentValues().apply { put("k", "stamp"); put("v", "2.44.1/2.44.1") })
        assertFalse(mirror.setHost("host", MirrorDatabase.FORMAT, "8"))
        mirror.apply(MirrorEvent.EntryChanged(entry(9)))
        // A host that sends no schema can't say it changed.
        assertFalse(mirror.setHost("host", MirrorDatabase.FORMAT))
        assertEquals(1, mirror.snapshot().entries.size)
        // A migration may have rewritten or removed rows in place.
        assertTrue(mirror.setHost("host", MirrorDatabase.FORMAT, "9"))
        assertTrue(mirror.snapshot().entries.isEmpty())
        assertEquals(0, mirror.snapshot().rev)
    }

    private fun hello(version: String, hostId: String = "host", iconsRevision: Long = 0) = buildJsonObject {
        put("hostId", hostId); put("version", version); put("iconsRevision", iconsRevision)
    }

    @Test fun anAppAndHostUpdateKeepTheCursorOfAMirrorAnOlderAppWrote() {
        mirror.apply(MirrorEvent.BotChanged(Bot("bot", "Bot", rev = 3)))
        mirror.apply(MirrorEvent.EntryChanged(chat("bot", 4)))
        // As 2.44.1 left it: no format, a stamp of its own version and the host's then.
        mirror.writableDatabase.apply {
            insertOrThrow("meta", null, ContentValues().apply { put("k", "hostId"); put("v", "host") })
            insertOrThrow("meta", null, ContentValues().apply { put("k", "stamp"); put("v", "2.44.1/2.44.1") })
        }
        val before = mirror.snapshot()
        assertEquals(4, before.rev)
        // The updated app's first connect() hello reports an updated host too: the conversation and cursor stay.
        assertFalse(mirror.connectedHello(hello("2.45.0", iconsRevision = 9), mirror.beginIconConnection()))
        assertEquals(before.rev, mirror.snapshot().rev)
        assertEquals(before.entries, mirror.snapshot().entries)
        assertFalse(mirror.connectedHello(hello("2.45.1"), mirror.beginIconConnection()))
        assertEquals(before.rev, mirror.snapshot().rev)
    }

    @Test fun aReconnectRetainsTheCommittedCursorAndHistoryFloor() {
        // First connect() hello, then a catch-up of thirty chats with days of messages each.
        assertFalse(mirror.connectedHello(hello("2.44.1"), mirror.beginIconConnection()))
        var rev = 0L
        val day = 1_700_000_000_000L
        for (b in 0 until 30) {
            mirror.apply(MirrorEvent.BotChanged(Bot("bot$b", "Bot $b", rev = ++rev)))
            // Seqs 2..5: seq 1 never reached this phone, so its floor is 2.
            for (t in 2..5) mirror.apply(MirrorEvent.EntryChanged(
                Entry("e-bot$b-$t", t.toLong(), "bot$b", rev = ++rev, kind = "user", createdAt = day + t * 86_400_000,
                    data = buildJsonObject { put("text", "m$t"); put("clientNonce", "n-bot$b-$t") })))
        }
        val head = rev
        assertEquals(head, mirror.revision())

        // The host updated in place and the link dropped: the reconnect's hello reports the new
        // version, but the cursor this connect passes to events() is the committed one.
        assertFalse(mirror.connectedHello(hello("2.45.0"), mirror.beginIconConnection()))
        assertEquals(head, mirror.revision())
        val before = mirror.snapshot(Residency(emptySet(), preview = 10)).entries
        assertEquals(120, before.size)

        // The reconnect's stream hello leads again; the gap it leaves (seq 1) is still paged, not hidden.
        mirror.apply(MirrorEvent.Hello("host", head + 2, JsonObject(emptyMap()), null))
        mirror.apply(MirrorEvent.EntryChanged(
            Entry("e-bot0-1", 1, "bot0", rev = head + 1, kind = "user",
                data = buildJsonObject { put("text", "rewritten"); put("clientNonce", "n-bot0-1") })))
        assertFalse("a row below the floor waits for history paging", mirror.hasNonce("bot0", "n-bot0-1"))
        // What the computer wrote while the phone was away lands from the cursor.
        mirror.apply(MirrorEvent.EntryChanged(
            Entry("e-bot0-6", 6, "bot0", rev = head + 2, kind = "user",
                data = buildJsonObject { put("text", "while away"); put("clientNonce", "n-bot0-6") })))
        val after = mirror.snapshot(Residency(setOf("bot0"), preview = 10, window = 10))
        assertEquals(head + 2, after.rev)
        assertEquals(listOf(2L, 3L, 4L, 5L, 6L), after.entries.filter { it.botId == "bot0" }.map { it.seq })
    }

    @Test fun interruptedSendRetainsItsNonceAndRequiresAnExplicitRetry() {
        mirror.save(PendingSend("stable", "bot", text = "message", createdAt = 1))
        mirror.close()
        mirror = MirrorDatabase(context, directory, computer)
        mirror.recoverInterrupted()
        assertEquals("stable", mirror.snapshot().pending.single().nonce)
        assertEquals(SendStatus.Failed, mirror.snapshot().pending.single().status)
    }

    @Test fun aLateSendFailureCannotResurrectAnAlreadyConfirmedMessage() {
        val send = PendingSend("stable", "bot", text = "message", createdAt = 1)
        mirror.save(send)
        mirror.apply(MirrorEvent.EntryChanged(entry(3, nonce = send.nonce)))
        mirror.save(send.copy(status = SendStatus.Failed))
        assertTrue(mirror.snapshot().pending.isEmpty())
        assertEquals(1, mirror.snapshot().entries.size)
    }

    @Test fun boundedSnapshotKeepsResidentChatsWholeAndPreviewsTheRest() {
        val keep = (1L..40L).map { chat("keep", it) }
        val previewed = (1L..40L).map { chat("drop", it) }
        val short = (1L..2L).map { chat("short", it) }
        mirror.receive(keep + previewed + short)
        mirror.save(PendingSend("unsent", "drop", text = "waiting", createdAt = 1))
        val everything = mirror.snapshot()
        val bounded = mirror.snapshot(Residency(full = setOf("keep"), preview = 30, window = 40))
        assertEquals((1L..40L).toList(), bounded.entries.filter { it.botId == "keep" }.map { it.seq })
        assertEquals((11L..40L).toList(), bounded.entries.filter { it.botId == "drop" }.map { it.seq })
        assertEquals(listOf(1L, 2L), bounded.entries.filter { it.botId == "short" }.map { it.seq })
        assertFalse(bounded.entries.any { it.clientNonce == "n-drop-1" })
        assertEquals(everything.rev, bounded.rev)
        assertEquals(listOf("unsent"), bounded.pending.map { it.nonce })
        assertEquals(setOf("n-drop-1"), mirror.confirmedNonces(listOf(ComposerDraft("drop", nonce = "n-drop-1"), ComposerDraft("drop", nonce = "never-sent"))))
        assertTrue(mirror.hasNonce("drop", "n-drop-1"))
        assertFalse(mirror.hasNonce("keep", "n-drop-1"))
        assertTrue(mirror.snapshot(Residency(emptySet(), preview = 0)).entries.isEmpty())
    }

    @Test fun aResidentChatLoadsOnlyItsWindowAndOlderRowsStayReachable() {
        mirror.receive((1L..50L).map { chat("big", it) })
        val windowed = mirror.snapshot(Residency(setOf("big"), preview = 5, window = 20))
        assertEquals((31L..50L).toList(), windowed.entries.map { it.seq })
        assertTrue(mirror.hasOlder("big", 31))
        assertFalse(mirror.hasOlder("big", 1))
        val grown = mirror.snapshot(Residency(setOf("big"), preview = 5, window = 20, windows = mapOf("big" to 45)))
        assertEquals((6L..50L).toList(), grown.entries.map { it.seq })
    }

    @Test fun anOldRowTheComputerRewritesDoesNotHideTheGapBeforeIt() {
        mirror.apply((100L..120L).map { MirrorEvent.EntryChanged(chat("big", it)) })
        // A send's reply stored after the cursor comes again in the catch-up: it sets no floor.
        mirror.receive(listOf(chat("quiet", 30).copy(rev = 128)))
        mirror.apply(MirrorEvent.Hello("host", 130, JsonObject(emptyMap()), null))
        mirror.apply(MirrorEvent.EntryChanged(chat("big", 5).copy(rev = 125)))
        assertFalse("a row below the oldest stored one waits to be paged in", mirror.hasNonce("big", "n-big-5"))
        assertEquals(100L, mirror.snapshot(Residency(setOf("big"), window = 50)).entries.filter { it.botId == "big" }.minOf { it.seq })
        mirror.apply(MirrorEvent.EntryChanged(chat("big", 121)))
        assertTrue(mirror.hasNonce("big", "n-big-121"))
        mirror.receive(listOf(chat("big", 5)))
        assertTrue("a history page may reach below the stored rows", mirror.hasNonce("big", "n-big-5"))
        mirror.apply(MirrorEvent.EntryChanged(chat("quiet", 25).copy(rev = 126)))
        assertTrue("a chat with no rows at the cursor takes whatever arrives", mirror.hasNonce("quiet", "n-quiet-25"))
    }

    @Test fun aCatchUpFromTheStartKeepsEveryRowWhateverItsOrder() {
        mirror.setHost("host", MirrorDatabase.FORMAT)
        mirror.apply((10L..20L).map { MirrorEvent.EntryChanged(chat("bot", it)) })
        // A new mirror format fetches the conversation again from rev 0, and a send's reply lands before the catch-up.
        assertTrue(mirror.setHost("host", MirrorDatabase.FORMAT + 1))
        mirror.receive(listOf(chat("bot", 30)))
        mirror.apply(MirrorEvent.Hello("host", 40, JsonObject(emptyMap()), null))
        // Sorted by rev, so an older row edited later arrives after a newer one.
        mirror.apply(listOf(4L, 2L, 7L).mapIndexed { i, seq -> MirrorEvent.EntryChanged(chat("bot", seq).copy(rev = 31L + i)) })
        assertEquals(listOf(2L, 4L, 7L, 30L), mirror.snapshot().entries.map { it.seq })
    }

    @Test fun anArtifactDraftIsConfirmedByItsChatsMessage() {
        mirror.receive(listOf(chat("bot", 1)))
        val artifact = ComposerDraft("bot/artifact/a1", nonce = "n-bot-1")
        val unsent = ComposerDraft("bot/artifact/a2", nonce = "never")
        assertEquals(setOf("n-bot-1"), mirror.confirmedNonces(listOf(artifact, unsent)))
    }

    @Test fun largeRowsStopAChatFromGrowingPastItsByteBudgetAndAPreviewKeepsItsNewestRow() {
        val filler = "x".repeat(60_000)
        fun big(seq: Long) = Entry("e-big-$seq", seq, "big", rev = seq, kind = "user",
            data = buildJsonObject { put("text", filler); put("clientNonce", "n-big-$seq") })
        mirror.receive((1L..600L).map(::big))
        val small = mirror.snapshot(Residency(setOf("big"), window = 300)).entries.filter { it.botId == "big" }
        val wide = mirror.snapshot(Residency(setOf("big"), window = 500, windows = mapOf("big" to 500))).entries.filter { it.botId == "big" }
        assertTrue("a chat of large rows stays inside its byte budget", small.size < 300)
        assertTrue("and a wider window can't grow past it", wide.size <= 24_000_000 / 60_000)
        assertEquals(600L, small.maxOf { it.seq })
        val preview = mirror.snapshot(Residency(emptySet(), window = 300, preview = 30)).entries.filter { it.botId == "big" }
        assertTrue("a preview of large rows is cut short but keeps the newest row", preview.size in 1..29)
        assertEquals(600L, preview.maxOf { it.seq })
    }

    private fun chat(bot: String, seq: Long): Entry = Entry("e-$bot-$seq", seq, bot, rev = seq, kind = "user",
        data = buildJsonObject { put("text", "$bot-$seq"); put("clientNonce", "n-$bot-$seq") })

    @Test fun upgradingTheMirrorFetchesTheConversationAgainAndKeepsUnsentMessages() {
        mirror.close()
        val path = File(directory, "mirror-$computer.db")
        path.delete()
        SQLiteDatabase.openOrCreateDatabase(path, null).use { db ->
            db.execSQL("CREATE TABLE meta(k TEXT PRIMARY KEY, v TEXT NOT NULL)")
            db.execSQL("CREATE TABLE bots(id TEXT PRIMARY KEY, rev INTEGER NOT NULL, deleted INTEGER NOT NULL, payload TEXT NOT NULL)")
            db.execSQL("CREATE TABLE entries(id TEXT PRIMARY KEY, bot_id TEXT NOT NULL, thread_id TEXT, seq INTEGER NOT NULL, rev INTEGER NOT NULL, nonce TEXT, payload TEXT NOT NULL)")
            db.execSQL("CREATE TABLE pending(nonce TEXT PRIMARY KEY, created INTEGER NOT NULL, payload TEXT NOT NULL)")
            db.execSQL("CREATE TABLE history(lane TEXT PRIMARY KEY)")
            db.insertOrThrow("meta", null, ContentValues().apply { put("k", "rev"); put("v", "3") })
            db.insertOrThrow("entries", null, ContentValues().apply {
                put("id", "message"); put("bot_id", "bot"); put("thread_id", "root"); put("seq", 1); put("rev", 3)
                put("payload", WireJson.encodeToString(entry(3)))
            })
            db.insertOrThrow("pending", null, ContentValues().apply {
                put("nonce", "unsent"); put("created", 1)
                put("payload", """{"nonce":"unsent","botId":"bot","threadId":"root","text":"aside","createdAt":1}""")
            })
            db.insertOrThrow("history", null, ContentValues().apply { put("lane", "bot") })
            db.version = 2
        }
        mirror = MirrorDatabase(context, directory, computer)
        val snapshot = mirror.snapshot()
        assertTrue(snapshot.entries.isEmpty())
        assertEquals(0, snapshot.rev)
        assertFalse(mirror.historyComplete("bot"))
        assertEquals("aside", snapshot.pending.single().text)
        mirror.apply(MirrorEvent.EntryChanged(entry(4, nonce = "unsent")))
        assertTrue(mirror.snapshot().pending.isEmpty())
    }
}
