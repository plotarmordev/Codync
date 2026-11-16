package com.codync.android

import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

/** A phone's open-timing measurement for its connected host's local diagnostic log.
 * Dedupe keys stay on the phone: no computer, bot or message identity is sent. */
internal data class PhonePerformance(val phase: OpenPhase, val open: OpenKind, val ms: Long,
    val chats: Int? = null, val messages: Int? = null) {
    fun body(): JsonObject = buildJsonObject {
        put("type", "open_timing")
        put("platform", "android")
        put("phase", phase.wire)
        put("open", open.wire)
        put("ms", ms)
        chats?.let { put("chats", it) }
        messages?.let { put("messages", it) }
    }
}

internal enum class OpenPhase(val wire: String) {
    LinkReady("link_ready"), Hello("hello"), CatchUp("catch_up"), ChatList("chat_list"), OpenChat("open_chat"),
}

internal enum class OpenKind(val wire: String) { Cold("cold"), Foreground("foreground") }
