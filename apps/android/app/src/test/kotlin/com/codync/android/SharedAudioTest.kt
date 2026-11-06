package com.codync.android

import java.io.File
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.yield
import org.junit.Assert.*
import org.junit.Test

/** A voice note's player (iOS `SharedAudioTests`): its fetch from the computer is the test's to answer. */
class SharedAudioTest {
    private class Speaker : Sound {
        val log = mutableListOf<String>()
        override var playing = false
        override fun start(): Boolean { log += "start"; playing = true; return true }
        override fun pause() { log += "pause"; playing = false }
        override fun release() { log += "release"; playing = false }
    }

    private val speaker = Speaker()
    private val fetch = CompletableDeferred<File>()
    private var loads = 0
    private val audio = SharedAudio(load = { loads++; fetch.await() }, open = { _, _ -> speaker.log += "open"; speaker },
        failed = { speaker.log += "failed" })

    @Test fun `a second tap while the sound loads plays it once`() = runBlocking {
        val first = launch { audio.toggle() }
        yield()
        audio.toggle()
        assertEquals(1, loads)
        fetch.complete(File("note.m4a"))
        first.join()
        assertEquals(listOf("open", "start"), speaker.log)
        assertTrue(audio.playing)
        audio.toggle()
        assertFalse(audio.playing)
    }

    @Test fun `a card released while its sound loads plays nothing`() = runBlocking {
        val tap = launch { audio.toggle() }
        yield()
        audio.release()
        fetch.complete(File("note.m4a"))
        tap.join()
        assertEquals(emptyList<String>(), speaker.log)
        assertFalse(audio.playing)
    }

    @Test fun `an unreadable sound says so`() = runBlocking {
        val tap = launch { audio.toggle() }
        yield()
        fetch.completeExceptionally(java.io.IOException("gone"))
        tap.join()
        assertEquals(listOf("failed"), speaker.log)
    }
}
