import Foundation
import Testing
@testable import CodyncKit
@testable import CodyncUI

private func entry(_ id: String, kind: String, data: String, at: Int64, botId: String = "task-1") throws -> Entry {
    let json = #"{"id":"\#(id)","seq":\#(at),"botId":"\#(botId)","rev":1,"kind":"\#(kind)","turn":1,"data":\#(data),"createdAt":\#(at),"updatedAt":1}"#
    return try JSONDecoder().decode(Entry.self, from: Data(json.utf8))
}

/// A thread's chat (Android's `ThreadChatItemsTest`): what it was told on top, its messages and what it put in the
/// bot's chat, without its steps or, while it works, the user's waiting messages.
@Test func aThreadsChatIsWhatItWasToldItsMessagesAndWhatItPutInTheChat() throws {
    let ids = { (items: [ChatItem]) in items.compactMap { if case let .entry(e, _) = $0.kind { e.id } else { nil } } }
    let transcript = [
        try entry("told", kind: "user", data: #"{"text":"Fix login","from":"host"}"#, at: 10),
        try entry("tool", kind: "tool", data: "{}", at: 20),
        try entry("thought", kind: "thought", data: #"{"text":"Hmm"}"#, at: 21),
        try entry("reply", kind: "agent", data: #"{"text":"Done","final":true}"#, at: 30),
        try entry("mine", kind: "user", data: #"{"text":"Also the tests","status":"queued"}"#, at: 40),
    ].threadEntries
    let card = try entry("card", kind: "permission", data: #"{"status":"pending","task":{"id":"task-1","title":"Fix login"}}"#, at: 25, botId: "b")
    let chatOnly = try entry("chat", kind: "agent", data: #"{"text":"Hi","final":true}"#, at: 26, botId: "b")

    #expect(ids(ChatItem.thread(transcript, chat: [card, chatOnly], task: "task-1", working: true)) == ["told", "card", "reply"])
    #expect(ids(ChatItem.thread(transcript, chat: [card, chatOnly], task: "task-1", working: false)) == ["told", "card", "reply", "mine"])
    // An item no task has taken has nothing of the chat's.
    #expect(ids(ChatItem.thread(transcript, chat: [card], task: nil, working: false)) == ["told", "reply", "mine"])
}
