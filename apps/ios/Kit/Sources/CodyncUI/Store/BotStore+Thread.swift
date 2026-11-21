import CodyncKit
import Foundation

/// Writing to an item's thread: its messages are entries of its task's transcript (`taskEntries`), never a chat's.
extension BotStore {
    /// A task's entry as it came in. The computer's copy of a message sent from here replaces the one shown at once.
    func putTaskEntry(_ e: Entry, task: String) {
        // ponytail: re-sorts the task's transcript on every entry; insert in place if tasks grow past hundreds.
        taskEntries[task] = (taskEntries[task] ?? []).mergingTaskEntries([e])
    }

    func dropTaskEntry(_ id: String, task: String) {
        taskEntries[task]?.removeAll { $0.id == id }
    }

    /// A message for an item's thread, with its files as in the chat: in its transcript at once, then as the computer
    /// holds it (waiting behind the task's turn, or starting one). A refusal leaves it failed and says why.
    func send(_ text: String, to thread: WorkThread, files: [OutgoingFile] = [], nonce: String = UUID().uuidString) {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty || !files.isEmpty, !retired else { return }
        lastSentNonce = nonce
        var data = EntryData(text: trimmed, status: "sending", clientNonce: nonce)
        if !files.isEmpty {
            data.attachments = files.map { Attachment(id: $0.id.uuidString, name: $0.name, size: Int64($0.data.count)) }
            outgoingFiles[nonce] = files
        }
        var local = Entry(id: Entry.localId(nonce), seq: .max, botId: thread.task, rev: 0, kind: "user", turn: 0,
                          data: data, createdAt: Int64(Date.now.timeIntervalSince1970 * 1000), updatedAt: 0)
        putTaskEntry(local, task: thread.task)
        Task {
            do {
                // Safe to repeat: the computer answers a nonce it already has with the same entry.
                let e = try await withLink(replay: true) { client in
                    let ids = try await self.uploadOutgoing(nonce, botId: thread.botId, itemId: thread.itemId, with: client)
                    return try await client.send(botId: thread.botId, text: trimmed, clientNonce: nonce, attachments: ids,
                                                 itemId: thread.itemId)
                }
                outgoingFiles[nonce] = nil
                putTaskEntry(e, task: thread.task)
            } catch {
                // A dropped link: the message's own "Failed to send" and Resend say enough.
                if (error as? HostError)?.isTransient != true { lastError = error.localizedDescription }
                // Unless the computer's copy came in on the events stream after all.
                guard taskEntries[thread.task]?.contains(where: { $0.id == local.id }) == true else { return }
                local.data.status = "failed"
                putTaskEntry(local, task: thread.task)
            }
        }
    }

    /// Resend on a message that failed here, with the `files` this app still holds for it; one the computer failed (a
    /// restart) goes again as a new message.
    func resend(_ entry: Entry, to thread: WorkThread, files: [OutgoingFile]) {
        let local = entry.data.clientNonce.map(Entry.localId) == entry.id
        if local { dropTaskEntry(entry.id, task: thread.task) }
        send(entry.data.text ?? "", to: thread, files: files, nonce: (local ? entry.data.clientNonce : nil) ?? UUID().uuidString)
    }

    /// Has the bot's conversation open its details on `item`'s thread once it is on screen.
    public func requestThread(of botId: String, item: WorkItem) { threadRequest = (botId, item) }

    /// Consumed once, including when the conversation was already on screen.
    func takeThreadRequest(for botId: String) -> WorkItem? {
        guard let request = threadRequest, request.botId == botId else { return nil }
        threadRequest = nil
        return request.item
    }
}
