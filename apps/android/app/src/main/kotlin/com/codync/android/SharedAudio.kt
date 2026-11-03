package com.codync.android

import android.media.MediaPlayer
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import com.codync.android.core.Entry
import com.codync.android.core.SharedFile
import com.codync.android.design.*
import java.io.IOException
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.launch

/** Plays a sound a bot shared (a voice note), fetched once from the computer (iOS `SharedAudio`); one card's player. */
private class SharedAudio {
    var playing by mutableStateOf(false)
        private set
    private var player: MediaPlayer? = null
    /** Taps arrive on the main thread: one while the sound loads is dropped, so it never starts a second player. */
    private var loading = false

    suspend fun toggle(entry: Entry, file: SharedFile, store: AppStore) {
        val current = player
        if (current != null && current.isPlaying) {
            current.pause()
            playing = false
            return
        }
        if (loading) return
        loading = true
        try {
            val ready = current ?: MediaPlayer().also { player = it }.apply {
                setDataSource(store.sharedFile(entry.id, file).path)
                setOnCompletionListener { playing = false }
                prepare()
            }
            ready.start()
            playing = true
        } catch (cancelled: CancellationException) { throw cancelled }
        catch (_: Exception) { release(); store.showError(IOException("Couldn't play ${file.name}.")) }
        finally { loading = false }
    }

    fun release() {
        player?.release()
        player = null
        playing = false
    }
}

/** A voice note's play/pause, in place of its file icon. */
@Composable internal fun SharedAudioButton(entry: Entry, file: SharedFile, store: AppStore) {
    val audio = remember(file.id) { SharedAudio() }
    val scope = rememberCoroutineScope()
    DisposableEffect(audio) { onDispose { audio.release() } }
    Box(Modifier.size(36.dp).plainButton { scope.launch { audio.toggle(entry, file, store) } }
        .semantics { contentDescription = (if (audio.playing) "Pause " else "Play ") + file.name }, contentAlignment = Alignment.Center) {
        Symbol(if (audio.playing) "pause.circle" else "play.circle", size = 24.dp, tint = Palette.text)
    }
}
