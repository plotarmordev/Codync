package com.codync.android

import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.withTimeoutOrNull
import com.codync.android.core.Login
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.put

// The user's hand on a bot's box: order, cutting in, stopping, dropping and trying again. The host decides what runs.

/** A waiting item moves to the front. */
internal fun AppStore.workFirst(botId: String, itemId: String) = changeWork("workFirst", botId, itemId)

/** A waiting item, or one that ended, leaves the box. */
internal fun AppStore.dropWork(botId: String, itemId: String) {
    mutableState.update { it.copy(olderThreads = it.olderThreads?.drop(botId, itemId)) }
    changeWork("dropWork", botId, itemId)
}

/** A working item's task stops. */
internal fun AppStore.stopWork(botId: String, itemId: String) = changeWork("stopWork", botId, itemId)

/** A stopped or failed item goes back to waiting, last. */
internal fun AppStore.retryWork(botId: String, itemId: String) = changeWork("retryWork", botId, itemId)

/** Starts [itemId] now in the place of the working item [insteadOf], which returns to the front and starts again later. */
internal fun AppStore.workCutIn(botId: String, itemId: String, insteadOf: String) =
    command("workCutIn", body("botId" to botId, "itemId" to itemId, "insteadOf" to insteadOf), workKey(botId))

private fun AppStore.changeWork(method: String, botId: String, itemId: String) =
    command(method, body("botId" to botId, "itemId" to itemId), workKey(botId))

/** Busy while one of [botId]'s work changes is out. */
internal fun workKey(botId: String) = "work:$botId"

/** How long a started thread may take to arrive through sync with its thread before its bot's details open instead (iOS `threadStarted`). */
private const val THREAD_START_MILLIS = 10_000L

/**
 * Starts a thread on [text] (`startThread`): with [botId] on the bot's own settings, or with none on [setup]; a failure
 * goes to [failed], where it was written. Its item arrives through sync: once it has a task, [done] gets its thread
 * chat; one with none by then (it waits behind others, or never came) opens its bot's details, and [done] gets null.
 */
internal fun AppStore.startThread(botId: String?, text: String, setup: JsonObject?, failed: (String) -> Unit, done: (ThreadRef?) -> Unit) =
    action(startThreadKey(botId)) {
        val request = buildJsonObject {
            botId?.let { put("botId", it) }
            put("text", text.trim())
            setup?.let { put("setup", it) }
        }
        val reply = try { ready().call("startThread", request).jsonObject }
            catch (cancelled: CancellationException) { throw cancelled }
            catch (failure: Exception) { return@action failed(failure.message ?: "Couldn't complete the action.") }
        val owner = reply.text("botId") ?: botId ?: return@action done(null)
        val itemId = reply.text("itemId")
        val threaded = itemId != null && withTimeoutOrNull(THREAD_START_MILLIS) {
            state.first { now -> now.bots.any { bot -> bot.id == owner && bot.work.any { it.id == itemId && it.task != null } } }
        } != null
        // Sent without a bot: the box the reply names, which may not have synced in yet.
        if (threaded && itemId != null) return@action done(ThreadRef(state.value.computer.id, owner, itemId))
        if (botId != null) openDetails(owner)
        done(null)
    }

/** Busy while a thread with [botId] (null: without a bot) is being started. */
internal fun startThreadKey(botId: String?) = "startThread:${botId ?: ""}"

/**
 * What a thread without a bot runs on (desktop `ThreadStart`): [login]'s agent, in [home], on [model], asking before
 * anything it does in the agent's Auto mode, as a new bot does.
 */
internal fun plainSetup(login: Login, home: String, model: String?, options: Map<String, String>): JsonObject = buildJsonObject {
    put("backend", login.backend)
    put("loginProfile", login.loginProfile.orEmpty())
    if (home.isNotEmpty()) put("cwd", home)
    put("model", buildJsonObject {
        model?.let { put("id", it) }
        put("options", buildJsonObject { options.forEach { (key, value) -> put(key, value) } })
    })
    put("approval", buildJsonObject { put("permission", "ask"); put("mode", "auto") })
}
