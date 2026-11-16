package com.codync.android

import android.os.SystemClock
import kotlin.time.Duration
import kotlin.time.Duration.Companion.milliseconds

/**
 * Milliseconds from each cold or foreground open to its connection and rendering milestones
 * (iOS `OpenTiming`). Main thread only; one [AnalyticsEvent.OpenTiming] per phase, computer and open.
 */
internal class OpenTiming(
    private val now: () -> Duration = { SystemClock.elapsedRealtime().milliseconds },
    private val capture: (AnalyticsEvent) -> Unit = Analytics::capture,
) {
    private var openedAt: Duration? = null
    private var kind = OpenKind.Cold
    private var backgrounded = false
    private var computers = 0
    private val sent = mutableSetOf<Pair<String, OpenPhase>>()
    /** Opens so far: a store reports its milestones once it has noticed the current one. */
    var opens = 0
        private set

    /** Starts before the saved computers open their links (iOS `AppStore.init`). */
    fun beginCold() = begin(OpenKind.Cold)

    /** The open's computer count once the saved computers are known (they load after `becameActive`). */
    fun setComputerCount(count: Int) {
        computers = count
    }

    fun becameActive(computers: Int) {
        this.computers = computers
        if (backgrounded) begin(OpenKind.Foreground)
        backgrounded = false
    }

    fun enteredBackground() {
        backgrounded = true
    }

    private fun begin(open: OpenKind) {
        openedAt = now()
        kind = open
        sent.clear()
        opens += 1
    }

    /** Whether [phase] already went out for [computer] this open (iOS `hasRecorded`). */
    fun hasRecorded(phase: OpenPhase, computer: String): Boolean = computer to phase in sent

    /** A computer reached [phase]; [chats]/[messages] are `catch_up`'s counts. Once per phase and computer per open. */
    fun record(phase: OpenPhase, computer: String, chats: Int? = null, messages: Int? = null) {
        val opened = openedAt ?: return
        if (!sent.add(computer to phase)) return
        capture(AnalyticsEvent.OpenTiming(phase, kind, (now() - opened).inWholeMilliseconds.coerceAtLeast(0), computer, computers, chats, messages))
    }

    companion object {
        val shared = OpenTiming()
    }
}
