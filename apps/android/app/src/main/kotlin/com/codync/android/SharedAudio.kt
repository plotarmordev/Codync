package com.codync.android

import android.media.AudioAttributes
import android.media.AudioFocusRequest
import android.media.AudioManager
import android.media.MediaMetadataRetriever
import android.media.MediaPlayer
import android.net.ConnectivityManager
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectHorizontalDragGestures
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.requiredHeight
import androidx.compose.foundation.layout.requiredSize
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.CustomAccessibilityAction
import androidx.compose.ui.semantics.ProgressBarRangeInfo
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.SemanticsPropertyReceiver
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.customActions
import androidx.compose.ui.semantics.onClick
import androidx.compose.ui.semantics.progressBarRangeInfo
import androidx.compose.ui.semantics.setProgress
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import com.codync.android.core.Entry
import com.codync.android.core.SharedFile
import com.codync.android.design.*
import com.kyant.shapes.Capsule
import java.io.File
import java.io.IOException
import kotlin.coroutines.resume
import kotlin.coroutines.resumeWithException
import kotlin.math.sign
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Deferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.async
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withContext

/** The state of a voice note's player, shown by its row's play button and caption. */
enum class AudioPhase { Idle, Loading, Playing, Paused, Failed }

/** Why a sound stopped on its own: played to its end, lost the audio focus, or broke while decoding. */
internal enum class SoundStop { Ended, Interrupted, Broke }

/** A loaded sound, ready to play (`MediaSound` in the app). */
internal interface Sound {
    val playing: Boolean
    /** Seconds long. */
    val duration: Double
    /** Seconds played so far. */
    val position: Double
    /** Whether it started (another app may hold the audio focus). */
    fun start(): Boolean
    fun pause()
    /** Moves its playhead (seconds), playing or paused. */
    fun seek(seconds: Double)
    fun release()
}

/**
 * Plays a sound a bot shared (a voice note), fetched once from the computer by [load] (iOS `SharedAudio`); one card's
 * player. [open] prepares the player and calls back when it stops on its own.
 * [knownDuration] is the length the computer stamped at share time (seconds), so the bar and clock are right from
 * the first frame; 0.0 (an older computer, an unreadable header) means unknown, until [preload] reads it with [length]
 * off the fetched bytes.
 */
internal class SharedAudio(
    private val load: suspend () -> File,
    private val open: suspend (File, stopped: (SoundStop) -> Unit) -> Sound,
    /** Where the one fetch runs: the card's, so a preload or tap that stops waiting leaves it to the others. */
    private val scope: CoroutineScope,
    private var knownDuration: Double = 0.0,
    private val length: suspend (File) -> Double = ::headerLength,
) {
    var phase by mutableStateOf(AudioPhase.Idle)
        private set
    /** Seconds long: [knownDuration] before the sound loads, its own length after. */
    var duration by mutableStateOf(knownDuration)
        private set
    /** Seconds played so far; the card's progress ticker updates it while it plays. */
    var elapsed by mutableStateOf(0.0)
        private set
    private var sound: Sound? = null
    /** The step this tap is on, so a failure says which one broke. */
    var failure = VoiceNoteFailure.Load
        private set
    /** The card went: a load still running then starts nothing. */
    private var released = false
    /** The one fetch of the sound's bytes preload, length reading and a play tap share; a failed one is forgotten. */
    private var fetching: Deferred<Result<File>>? = null

    suspend fun toggle() {
        val current = sound
        if (current != null && current.playing && phase == AudioPhase.Playing) {
            current.pause()
            elapsed = current.position
            phase = AudioPhase.Paused
            return
        }
        // Taps arrive on the main thread: one while the sound loads is dropped, so it never starts a second player.
        if (phase == AudioPhase.Loading) return
        val fromStart = phase == AudioPhase.Idle || phase == AudioPhase.Failed
        phase = AudioPhase.Loading
        failure = VoiceNoteFailure.Load
        try {
            val ready = current ?: bytes().let { file ->
                if (released) return
                open(file) { stop ->
                    when (stop) {
                        SoundStop.Ended -> finished()
                        SoundStop.Interrupted -> if (phase == AudioPhase.Playing) interrupted()
                        SoundStop.Broke -> broke()
                    }
                }.also {
                    if (released) { it.release(); return }
                    sound = it
                    duration = it.duration
                }
            }
            failure = VoiceNoteFailure.Playback
            if (ready.start()) { elapsed = ready.position; phase = AudioPhase.Playing } else { drop(); phase = AudioPhase.Failed }
        } catch (cancelled: CancellationException) {
            // The tap's scope went (a dismissed preview) while this card's player lives on: the next tap starts over.
            if (phase == AudioPhase.Loading) phase = if (sound != null) AudioPhase.Paused else AudioPhase.Idle
            throw cancelled
        }
        catch (_: Exception) { drop(); phase = AudioPhase.Failed }
        if (fromStart) Analytics.capture(AnalyticsEvent.VoiceNotePlayed(failure.takeIf { phase == AudioPhase.Failed }))
    }

    /** The computer's length arriving after the card mounted (a backfilled note): shown until the sound says. */
    fun setKnownDuration(seconds: Double) {
        if (seconds <= 0.0 || duration > 0.0) return
        knownDuration = seconds
        duration = seconds
    }

    /** Ready the sound before a tap asks for it: fetches its bytes (the fetch [toggle] would share), and while the
     *  length is unknown reads it off them, so the clock is there before the first play. A fetch that fails stays
     *  quiet: it is forgotten and the tap retries and reports it. */
    suspend fun preload() {
        val file = try { bytes() } catch (cancelled: CancellationException) { throw cancelled } catch (_: Exception) { return }
        if (duration > 0.0 || released) return
        val seconds = try { length(file) } catch (cancelled: CancellationException) { throw cancelled } catch (_: Exception) { return }
        if (!released) setKnownDuration(seconds)
    }

    /** The sound's bytes from the computer, one fetch for everyone asking: it runs in the card's [scope], so a caller
     *  cancelled while waiting (a preload restarting) doesn't fail it for the others, and it holds a [Result] so a
     *  failed fetch doesn't cancel the card's scope; one after it landed reuses it, and a failed one is dropped so the
     *  next ask fetches again. */
    private suspend fun bytes(): File {
        val running = fetching ?: scope.async { runCatching { load() } }.also { fetching = it }
        return running.await().onFailure { if (fetching === running) fetching = null }.getOrThrow()
    }

    /** Moves a loaded sound's playhead, held to its length; playing or paused, it stays so. */
    fun seek(seconds: Double) {
        val current = sound ?: return
        if (duration <= 0.0) return
        elapsed = seconds.coerceIn(0.0, duration)
        current.seek(elapsed)
    }

    /** Played to the end: back to a replayable start, the duration kept. */
    private fun finished() {
        elapsed = 0.0
        phase = AudioPhase.Idle
    }

    /** Lost the audio focus to another app: kept, paused. */
    private fun interrupted() {
        elapsed = sound?.position ?: 0.0
        phase = AudioPhase.Paused
    }

    /** Broke while playing: dropped, so the next tap fetches and decodes again. */
    private fun broke() {
        drop()
        phase = AudioPhase.Failed
    }

    fun release() {
        released = true
        drop()
    }

    private fun drop() {
        sound?.release()
        sound = null
        fetching = null
        duration = knownDuration
        elapsed = 0.0
    }

    /** The card's progress ticker keeps [elapsed] honest. */
    fun syncElapsed() {
        sound?.let { if (it.playing || phase == AudioPhase.Paused) elapsed = it.position }
    }
}

/** A sound's length (seconds) from its headers, without preparing a player; 0.0 when they don't say. */
private suspend fun headerLength(file: File): Double = withContext(Dispatchers.IO) {
    MediaMetadataRetriever().use {
        it.setDataSource(file.path)
        (it.extractMetadata(MediaMetadataRetriever.METADATA_KEY_DURATION)?.toLongOrNull() ?: 0L) / 1000.0
    }
}

/** A voice note is speech played as media (the media volume, not the ringer's). Lazy: `AudioAttributes` is Android's,
 *  so building it at class load would break the player helpers' unit tests. */
private val voiceNoteAudio by lazy {
    AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_MEDIA)
        .setContentType(AudioAttributes.CONTENT_TYPE_SPEECH).build()
}

/** A voice note through `MediaPlayer`, holding the audio focus while it plays as iOS holds its audio session. */
private class MediaSound(private val manager: AudioManager, private val player: MediaPlayer,
                         private val stopped: (SoundStop) -> Unit) : Sound {
    /** Other apps' sound pauses while the note plays and picks up again after. */
    private val focus = AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN_TRANSIENT).setAudioAttributes(voiceNoteAudio)
        .setOnAudioFocusChangeListener { if (it != AudioManager.AUDIOFOCUS_GAIN) { pause(); stopped(SoundStop.Interrupted) } }.build()

    init {
        player.setOnCompletionListener { manager.abandonAudioFocusRequest(focus); stopped(SoundStop.Ended) }
        player.setOnErrorListener { _, _, _ -> manager.abandonAudioFocusRequest(focus); stopped(SoundStop.Broke); true }
    }

    override val playing get() = player.isPlaying
    override val duration get() = player.duration / 1000.0
    override val position get() = runCatching { player.currentPosition / 1000.0 }.getOrDefault(0.0)

    override fun start(): Boolean {
        if (manager.requestAudioFocus(focus) != AudioManager.AUDIOFOCUS_REQUEST_GRANTED) return false
        player.start()
        return true
    }

    override fun pause() {
        if (player.isPlaying) player.pause()
        manager.abandonAudioFocusRequest(focus)
    }

    override fun seek(seconds: Double) = player.seekTo((seconds * 1000).toLong(), MediaPlayer.SEEK_CLOSEST)

    override fun release() {
        player.release()
        manager.abandonAudioFocusRequest(focus)
    }

    companion object {
        /** Decodes [file] off the main thread (`prepareAsync`), so a long recording never freezes the chat. */
        suspend fun open(manager: AudioManager, file: File, stopped: (SoundStop) -> Unit): MediaSound {
            val player = MediaPlayer()
            try {
                player.setAudioAttributes(voiceNoteAudio)
                player.setDataSource(file.path)
                suspendCancellableCoroutine<Unit> { prepared ->
                    player.setOnPreparedListener { prepared.resume(Unit) }
                    player.setOnErrorListener { _, what, extra ->
                        prepared.resumeWithException(IOException("Couldn't open the voice note ($what, $extra)")); true
                    }
                    player.prepareAsync()
                }
            } catch (error: Throwable) { player.release(); throw error }
            return MediaSound(manager, player, stopped)
        }
    }
}

/** One card's player, remembered by the file and released with it; [SharedFile.durationMs] is its known length.
 *  [online]: the computer is connected, so the note's sound can be fetched ahead of a tap. */
@Composable internal fun rememberSharedAudio(entry: Entry, file: SharedFile, store: AppStore, online: Boolean): SharedAudio {
    val context = LocalContext.current
    val manager = context.getSystemService(AudioManager::class.java)
    val scope = rememberCoroutineScope()
    val audio = remember(file.id) {
        SharedAudio(load = { store.sharedFile(entry.id, file) },
            open = { path, stopped -> MediaSound.open(manager, path, stopped) },
            scope = scope,
            knownDuration = (file.durationMs ?: 0L) / 1000.0)
    }
    DisposableEffect(audio) { onDispose { audio.release() } }
    // A backfilled note's length arrives after the card mounted: show it without waiting for the sound to load. The
    // sound itself preloads so a tap plays at once — unless this network is metered and the note is a big one, and
    // never while the computer is away (a chat shown before it connects fetches nothing).
    LaunchedEffect(audio, file.durationMs, online) {
        audio.setKnownDuration((file.durationMs ?: 0L) / 1000.0)
        val metered = context.getSystemService(ConnectivityManager::class.java)?.isActiveNetworkMetered ?: true
        if (voiceNotePreloads(online, metered, file.size)) audio.preload()
    }
    return audio
}

/** The sound's bytes big enough to skip on a network that charges for them (2 MiB; iOS `preloadLimit`). */
private const val PRELOAD_LIMIT = 2 * 1024 * 1024L

/** Whether the bubble preloads a sound as it shows: the computer online, and the fetch never costly — under
 *  [PRELOAD_LIMIT] even on a metered network (cellular, a hotspot; iOS `preloads`). */
internal fun voiceNotePreloads(online: Boolean, metered: Boolean, bytes: Long): Boolean =
    online && (!metered || bytes < PRELOAD_LIMIT)

/** A voice note's play/pause/loading, a plain glyph over a soft chip that fades in while pressed
 *  (a phone has no hover): a 48 dp target over a 24 dp glyph. */
@Composable internal fun SharedAudioButton(audio: SharedAudio, tapped: () -> Unit) {
    val source = remember { MutableInteractionSource() }
    val pressed by source.collectIsPressedAsState()
    val chip by animateFloatAsState(if (pressed) 0.14f else 0f, Motion.press(), label = "Play pressed")
    Box(Modifier.size(24.dp), contentAlignment = Alignment.Center) {
        Box(Modifier.requiredSize(48.dp).clickable(source, null, role = Role.Button, onClick = tapped),
            contentAlignment = Alignment.Center) {
            Box(Modifier.size(36.dp).clip(CircleShape).background(Palette.text.copy(alpha = chip)))
            when (audio.phase) {
                AudioPhase.Loading -> Spinner(24.dp, color = Palette.text)
                AudioPhase.Playing -> Symbol("pause.circle", size = 24.dp, tint = Palette.text)
                else -> Symbol("play.circle", size = 24.dp, tint = Palette.text)
            }
        }
    }
}

/** The one voice-note player both bubbles share: the play button, the progress bar (empty until the length is known)
 *  and the caption; a failed play puts its error in the bar's place, a failed download only colours the clock.
 *  One accessibility element — "[label], <caption>" — whose buttons become named actions; download is in the bubble's menu. */
@Composable internal fun SharedAudioRow(entry: Entry, file: SharedFile, audio: SharedAudio, label: String,
    state: AppState, store: AppStore) {
    val scope = rememberCoroutineScope()
    val translate = translator()
    val download = state.fileDownloads[file.id]
    val busy = download is FileDownloadState.Downloading
    val caption = voiceNoteDetail(download, audio, translate)
    val failed = audio.phase == AudioPhase.Failed
    val downloadFailed = download is FileDownloadState.Failed
    val play = { scope.launch { audio.toggle() }; Unit }
    Row(Modifier.fillMaxWidth().clearAndSetSemantics {
        contentDescription = if (caption.isEmpty()) label else "$label, $caption"
        audioSeek(audio)
        customActions = listOf(
            CustomAccessibilityAction(translate(if (audio.phase == AudioPhase.Playing) "Pause" else "Play")) { play(); true },
            CustomAccessibilityAction(translate(if (busy) "Cancel download" else "Download")) { store.toggleFile(entry.id, entry.botId, file); true })
        onClick(label = translate(if (audio.phase == AudioPhase.Playing) "Pause" else "Play")) { play(); true }
    }, verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        SharedAudioButton(audio, play)
        if (failed) {
            Text(caption, Modifier.weight(1f).padding(end = 12.dp), style = Type.caption2, color = Palette.danger, textAlign = TextAlign.Center)
        } else {
            AudioProgress(audio, Modifier.weight(1f))
            // The clock keeps the room of its duration's shape, with steady digits, so the bar ends in one place
            // from the first frame, when the length arrives, and as the clock runs.
            Box(contentAlignment = Alignment.CenterEnd) {
                Text(voiceNoteRoom(audio), Modifier.alpha(0f), style = Type.caption2.monospacedDigit)
                Text(caption, style = Type.caption2.monospacedDigit, color = if (downloadFailed) Palette.danger else Palette.secondary)
            }
        }
    }
}

/** The progress bar of a voice note's player, filled left to right, read again about four times a second while it
 *  plays; as wide as [modifier] lets it, the whole row by default. Tap or drag it to move there: its 48 dp touch strip
 *  overflows the 4 dp bar without taking up layout, as the play button's does. The strip is inert until the length is
 *  known, and a tap before the sound loads does nothing (only a loaded sound can seek). */
@Composable internal fun AudioProgress(audio: SharedAudio, modifier: Modifier = Modifier.fillMaxWidth()) {
    LaunchedEffect(audio, audio.phase) {
        audio.syncElapsed()
        while (audio.phase == AudioPhase.Playing) {
            delay(250)
            audio.syncElapsed()
        }
    }
    val seekTo = { x: Float, width: Int -> audio.seek(audio.duration * (x / width).coerceIn(0f, 1f)) }
    val strip = if (audio.duration > 0.0) Modifier
        .pointerInput(audio) { detectTapGestures { seekTo(it.x, size.width) } }
        .pointerInput(audio) { detectHorizontalDragGestures { change, _ -> seekTo(change.position.x, size.width) } }
    else Modifier
    Box(modifier.height(4.dp), contentAlignment = Alignment.CenterStart) {
        Box(Modifier.fillMaxWidth().requiredHeight(48.dp).then(strip), contentAlignment = Alignment.CenterStart) {
            Box(Modifier.fillMaxWidth().height(4.dp).clip(Capsule()).background(Palette.tertiary)) {
                val filled = if (audio.duration > 0.0) (audio.elapsed / audio.duration).toFloat().coerceIn(0f, 1f) else 0f
                Box(Modifier.fillMaxWidth(filled).height(4.dp).clip(Capsule()).background(Palette.text))
            }
        }
    }
}

/** Seconds TalkBack's adjust moves a voice note's playhead, as VoiceOver's on iOS and the arrow keys on desktop. */
private const val SEEK_STEP = 5.0

/** TalkBack reads a voice note's position and moves it five seconds (swipe up/down or the volume keys) once its length is known. */
internal fun SemanticsPropertyReceiver.audioSeek(audio: SharedAudio?) {
    if (audio == null || audio.duration <= 0.0) return
    val elapsed = audio.elapsed.toFloat()
    progressBarRangeInfo = ProgressBarRangeInfo(elapsed, 0f..audio.duration.toFloat())
    setProgress { audio.seek(seekStepTarget(audio.elapsed, it.compareTo(elapsed))); true }
}

/** Where an accessibility adjust toward `direction` (its sign; 0 at either end) moves a playhead at [elapsed]: one [SEEK_STEP]
 *  that way, whatever value the service asked for, so every note moves alike whatever its length. */
internal fun seekStepTarget(elapsed: Double, direction: Int): Double = elapsed + SEEK_STEP * direction.sign
