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
import java.io.File
import java.io.IOException
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.launch

/** A loaded sound, ready to play (`MediaSound` in the app). */
internal interface Sound {
    val playing: Boolean
    /** Whether it started (another app may hold the audio focus). */
    fun start(): Boolean
    fun pause()
    fun release()
}

/**
 * Plays a sound a bot shared (a voice note), fetched once from the computer by [load] (iOS `SharedAudio`); one card's
 * player. [open] makes the player and calls back when it stops on its own.
 */
internal class SharedAudio(
    private val load: suspend () -> File,
    private val open: (File, stopped: () -> Unit) -> Sound,
    private val failed: () -> Unit,
) {
    var playing by mutableStateOf(false)
        private set
    private var sound: Sound? = null
    /** Taps arrive on the main thread: one while the sound loads is dropped, so it never starts a second player. */
    private var loading = false
    /** The card went: a load still running then starts nothing. */
    private var released = false

    suspend fun toggle() {
        val current = sound
        if (current != null && current.playing) {
            current.pause()
            playing = false
            return
        }
        if (loading) return
        loading = true
        try {
            val ready = current ?: load().let { file ->
                if (released) return
                open(file) { playing = false }.also { sound = it }
            }
            playing = ready.start()
        } catch (cancelled: CancellationException) { throw cancelled }
        catch (_: Exception) { drop(); failed() }
        finally { loading = false }
    }

    fun release() {
        released = true
        drop()
    }

    private fun drop() {
        sound?.release()
        sound = null
        playing = false
    }
}

/** A voice note through `MediaPlayer`, holding the audio focus while it plays as iOS holds its audio session. */
private class MediaSound(private val manager: AudioManager, file: File, private val stopped: () -> Unit) : Sound {
    private val attributes = AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_MEDIA)
        .setContentType(AudioAttributes.CONTENT_TYPE_SPEECH).build()
    /** Other apps' sound pauses while the note plays and picks up again after. */
    private val focus = AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN_TRANSIENT).setAudioAttributes(attributes)
        .setOnAudioFocusChangeListener { if (it != AudioManager.AUDIOFOCUS_GAIN) { pause(); stopped() } }.build()
    private val player = MediaPlayer().apply {
        try {
            setAudioAttributes(attributes)
            setDataSource(file.path)
            setOnCompletionListener { manager.abandonAudioFocusRequest(focus); stopped() }
            prepare()
        } catch (error: Exception) { release(); throw error }
    }

    override val playing get() = player.isPlaying

    override fun start(): Boolean {
        if (manager.requestAudioFocus(focus) != AudioManager.AUDIOFOCUS_REQUEST_GRANTED) return false
        player.start()
        return true
    }

    override fun pause() {
        if (player.isPlaying) player.pause()
        manager.abandonAudioFocusRequest(focus)
    }

    override fun release() {
        player.release()
        manager.abandonAudioFocusRequest(focus)
    }
}

/** A voice note's play/pause, in place of its file icon. */
@Composable internal fun SharedAudioButton(entry: Entry, file: SharedFile, store: AppStore) {
    val manager = LocalContext.current.getSystemService(AudioManager::class.java)
    val audio = remember(file.id) {
        SharedAudio(load = { store.sharedFile(entry.id, file) }, open = { path, stopped -> MediaSound(manager, path, stopped) },
            failed = { store.showError(IOException("Couldn't play ${file.name}.")) })
    }
    val scope = rememberCoroutineScope()
    DisposableEffect(audio) { onDispose { audio.release() } }
    Box(Modifier.size(36.dp).plainButton { scope.launch { audio.toggle() } }
        .semantics { contentDescription = (if (audio.playing) "Pause " else "Play ") + file.name }, contentAlignment = Alignment.Center) {
        Symbol(if (audio.playing) "pause.circle" else "play.circle", size = 24.dp, tint = Palette.text)
    }
}
