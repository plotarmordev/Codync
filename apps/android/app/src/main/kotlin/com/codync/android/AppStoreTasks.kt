package com.codync.android

import com.codync.android.core.*
import java.util.UUID
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

// A task's own transcript, read and written to from its item's thread. Its entries never enter a chat
// (`AppState.entries`) nor the mirror: they are held per task id while the thread is open.

/**
 * A task's transcript opened: as the computer has it (newest 500), kept current from `taskEntry` events until
 * [closeTask].
 */
internal suspend fun AppStore.loadTask(botId: String, taskId: String) {
    // Held before the computer answers, so entries arriving meanwhile are kept and merged.
    mutableState.update { it.copy(taskEntries = it.taskEntries + (taskId to it.taskEntries[taskId].orEmpty())) }
    val entries = entriesOf(hostCall("taskEntries", body("botId" to botId, "taskId" to taskId), 20_000))
    mutableState.update { state ->
        val held = state.taskEntries[taskId] ?: return@update state
        state.copy(taskEntries = state.taskEntries + (taskId to mergeTaskEntries(held, entries)))
    }
}

/** The task's transcript closed: it is let go. */
internal fun AppStore.closeTask(taskId: String) = mutableState.update { it.copy(taskEntries = it.taskEntries - taskId) }

internal fun AppStore.receiveTasks(events: List<MirrorEvent>) = mutableState.update { state ->
    val received = state.taskEntries.receivingTasks(events)
    if (received === state.taskEntries) state else state.copy(taskEntries = received)
}

/**
 * The user writes [text] to the thread of [botId]'s item [itemId], with the files of its box's draft [draftKey], which
 * leaves the draft. It shows at once (`AppState.threadSends`), and one the computer refuses (an item that takes no
 * message, one that ended too long ago) stays with why, to send again or drop.
 */
internal fun AppStore.sendToThread(botId: String, itemId: String, draftKey: String, text: String) {
    val files = state.value.drafts[draftKey]?.files.orEmpty()
    if (text.isBlank() && files.isEmpty()) return
    val send = PendingSend(UUID.randomUUID().toString(), botId, text.trim(), System.currentTimeMillis(), files = files)
    lastSentNonce = send.nonce
    mutableState.update { it.copy(drafts = it.drafts - draftKey) }
    persistDrafts()
    deliverToThread(itemId, send)
}

/** Sends [send] to [itemId]'s thread, again too: the same nonce, so the computer never takes it twice. */
internal fun AppStore.deliverToThread(itemId: String, send: PendingSend) {
    mutableState.update { it.holdingThreadSend(itemId, send.copy(status = SendStatus.Sending, error = null)) }
    contextScope.launch {
        try {
            val connection = ready()
            val ids = uploadFiles(connection, send, itemId)
            val reply = connection.call("send", send.body(ids, itemId), 30_000)
            val entry = Entry.decode(reply.jsonObject.getValue("entry").jsonObject)
            mutableState.update { it.withoutThreadSend(itemId, send.nonce).receivingThreadEntry(entry) }
            withContext(Dispatchers.IO) { send.files.forEach(attachments::discard) }
        } catch (cancelled: CancellationException) { throw cancelled }
        catch (error: Exception) {
            mutableState.update { it.holdingThreadSend(itemId, send.copy(status = SendStatus.Failed, error = error.message ?: "Couldn't send.")) }
        }
    }
}

/** A message to [itemId]'s thread that didn't go is dropped, with the files it still holds. */
internal fun AppStore.discardThreadSend(itemId: String, nonce: String) {
    val files = state.value.threadSends[itemId].orEmpty().firstOrNull { it.nonce == nonce }?.files.orEmpty()
    mutableState.update { it.withoutThreadSend(itemId, nonce) }
    if (files.isNotEmpty()) contextScope.launch(Dispatchers.IO) { files.forEach(attachments::discard) }
}

/** A message waiting in [itemId]'s thread goes now, as a chat's does ([steer]); the caller names the bot's item, never its task. */
internal fun AppStore.steerThread(botId: String, itemId: String, entry: Entry) =
    command("steer", body("botId" to botId, "itemId" to itemId, "entryId" to entry.id), "steer:" + entry.id)

/** A message waiting in [itemId]'s thread never goes out: its row leaves when the computer says so (`removed`). */
internal fun AppStore.unqueueThread(botId: String, itemId: String, entry: Entry) =
    command("unqueue", body("botId" to botId, "itemId" to itemId, "entryId" to entry.id), "steer:" + entry.id)

/** [send] among [itemId]'s messages on their way, in the place it had. */
internal fun AppState.holdingThreadSend(itemId: String, send: PendingSend): AppState {
    val held = threadSends[itemId].orEmpty()
    val next = if (held.any { it.nonce == send.nonce }) held.map { if (it.nonce == send.nonce) send else it } else held + send
    return copy(threadSends = threadSends + (itemId to next))
}

internal fun AppState.withoutThreadSend(itemId: String, nonce: String): AppState {
    val rest = threadSends[itemId].orEmpty().filterNot { it.nonce == nonce }
    return copy(threadSends = if (rest.isEmpty()) threadSends - itemId else threadSends + (itemId to rest))
}

/** A thread's message as `send` answered it, in its task's transcript (the entry's `botId`) when that is open: there before its event. */
internal fun AppState.receivingThreadEntry(entry: Entry): AppState {
    val held = taskEntries[entry.botId] ?: return this
    return copy(taskEntries = taskEntries + (entry.botId to mergeTaskEntries(held, listOf(entry))))
}

/**
 * Reads the computer's next page of older tasks (`olderThreads`): the newest first, then the ones before the oldest held.
 * Nothing on a host without them, off the link, while a page loads or once none are left. A page that lands after the
 * list was dropped (a new hello, another Delete old threads) is let go.
 */
internal fun AppStore.loadOlderThreads() {
    val now = state.value
    if (threadKeepDays(now.hello) == null || now.actualConnection !is LinkState.Ready) return
    val held = now.olderThreads
    if (held?.more == false) return
    val host = now.hello?.text("hostId")
    action("olderThreads") {
        val reply = hostQuery("olderThreads", buildJsonObject { held?.before?.let { put("before", it) } })
        val page = OlderThreads.decode(reply.jsonObject)
        // Dropped meanwhile (a new hello, another Delete old threads): the page is let go.
        mutableState.update {
            if (it.hello?.text("hostId") != host || (held != null && it.olderThreads == null)) it
            else it.copy(olderThreads = (it.olderThreads ?: OlderThreads(more = true)) + page)
        }
    }
}

/** [bot] with its computer's older tasks it doesn't hold among its `work`, as the Threads list and a thread's page show them. */
internal fun AppState.withOlder(bot: Bot): Bot =
    olderThreads?.of(bot.id)?.takeIf { it.isNotEmpty() }?.let { bot.copy(work = mergedWork(bot.work, it)) } ?: bot
