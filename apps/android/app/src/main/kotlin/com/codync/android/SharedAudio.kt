package com.codync.android

import android.media.AudioAttributes
import android.media.AudioFocusRequest
import android.media.AudioManager
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
import androidx.compose.ui.platform.LocalContext
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
private class SharedAudio(private val manager: AudioManager) {
    var playing by mutableStateOf(false)
        private set
    private var player: MediaPlayer? = null
    /** Taps arrive on the main thread: one while the sound loads is dropped, so it never starts a second player. */
    private var loading = false
    private val attributes = AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_MEDIA)
        .setContentType(AudioAttributes.CONTENT_TYPE_SPEECH).build()
    /** Other apps' sound pauses while the note plays and picks up again after (iOS hands back its audio session). */
    private val focus = AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN_TRANSIENT).setAudioAttributes(attributes)
        .setOnAudioFocusChangeListener { if (it != AudioManager.AUDIOFOCUS_GAIN) pause() }.build()

    suspend fun toggle(entry: Entry, file: SharedFile, store: AppStore) {
        val current = player
        if (current != null && current.isPlaying) {
            pause()
            return
        }
        if (loading) return
        loading = true
        try {
            val ready = current ?: MediaPlayer().also { player = it }.apply {
                setAudioAttributes(attributes)
                setDataSource(store.sharedFile(entry.id, file).path)
                setOnCompletionListener { finished() }
                prepare()
            }
            if (manager.requestAudioFocus(focus) != AudioManager.AUDIOFOCUS_REQUEST_GRANTED) return
            ready.start()
            playing = true
        } catch (cancelled: CancellationException) { throw cancelled }
        catch (_: Exception) { release(); store.showError(IOException("Couldn't play ${file.name}.")) }
        finally { loading = false }
    }

    private fun pause() {
        player?.takeIf { it.isPlaying }?.pause()
        finished()
    }

    /** Paused, released or played to its end: other apps' sound may pick up again. */
    private fun finished() {
        playing = false
        manager.abandonAudioFocusRequest(focus)
    }

    fun release() {
        player?.release()
        player = null
        finished()
    }
}

/** A voice note's play/pause, in place of its file icon. */
@Composable internal fun SharedAudioButton(entry: Entry, file: SharedFile, store: AppStore) {
    val manager = LocalContext.current.getSystemService(AudioManager::class.java)
    val audio = remember(file.id) { SharedAudio(manager) }
    val scope = rememberCoroutineScope()
    DisposableEffect(audio) { onDispose { audio.release() } }
    Box(Modifier.size(36.dp).plainButton { scope.launch { audio.toggle(entry, file, store) } }
        .semantics { contentDescription = (if (audio.playing) "Pause " else "Play ") + file.name }, contentAlignment = Alignment.Center) {
        Symbol(if (audio.playing) "pause.circle" else "play.circle", size = 24.dp, tint = Palette.text)
    }
}
