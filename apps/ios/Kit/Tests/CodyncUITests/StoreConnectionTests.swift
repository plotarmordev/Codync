import CryptoKit
import Foundation
import Testing
@testable import CodyncKit
@testable import CodyncUI

@MainActor @Test func visibleConversationAcknowledgesEntriesBeforeUnreadAndFinalUpdates() async throws {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let fake = FakeRemote(.ready(.direct))
    let store = BotStore(computer: randomComputer("Mac"), storage: storage) { _ in fake }
    store.setActive(true)
    defer { store.setActive(false) }
    #expect(await until { await fake.subscribed })
    await fake.emit(botEvent("b1", name: "Bot", rev: 1))
    #expect(await until { store.connection == .online })
    await fake.emit(botEvent("b1", name: "Bot", rev: 1))
    #expect(await until { store.bots["b1"] != nil })
    let token = UUID()
    store.setReading(token, botId: "b1", active: true)
    #expect(await until { await fake.readReceipts.count == 1 })
    // Streaming and final content have the same ID. The final update must be read
    // even though the roster has not yet reported any unread messages.
    await fake.emit(#"{"type":"entry","entry":{"id":"answer","seq":1,"botId":"b1","rev":2,"kind":"agent","turn":1,"data":{"text":"draft","final":false},"createdAt":1,"updatedAt":1}}"#)
    #expect(await until { store.chat("b1").last?.rev == 2 })
    await fake.emit(#"{"type":"entry","entry":{"id":"answer","seq":1,"botId":"b1","rev":3,"kind":"agent","turn":1,"data":{"text":"done","final":true},"createdAt":1,"updatedAt":2}}"#)
    #expect(await until { await fake.readReceipts.count == 2 })
    await fake.emit(#"{"type":"bot","bot":{"id":"b1","name":"Bot","rev":4,"unread":1}}"#)
    #expect(await until { await fake.readReceipts.count == 3 })
    store.setReading(token, botId: "b1", active: false)
    await fake.emit(#"{"type":"bot","bot":{"id":"b1","name":"Bot","rev":5,"unread":2}}"#)
    #expect(await until { store.bots["b1"]?.rev == 5 })
    try await Task.sleep(for: .milliseconds(50))
    #expect(await fake.readReceipts.count == 3)
    // Returning to the foreground acknowledges without needing navigation.
    store.setReading(token, botId: "b1", active: true)
    #expect(await until { await fake.readReceipts.count == 4 })
}


@MainActor @Test func initialOfflineReportWaitsForReconnect() async throws {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let fake = FakeRemote(.hostOffline(lastSeen: nil))
    let store = BotStore(computer: randomComputer("Mac"), storage: storage) { _ in fake }
    defer { store.retire() }
    store.setActive(true)
    try await Task.sleep(for: .milliseconds(200))
    #expect(store.connection == .connecting)
    #expect(store.lastError == nil)
    await fake.set(.ready(.relay))
    #expect(await until { await fake.subscribed })
    await fake.emit(botEvent("b1", name: "Bot", rev: 1))
    #expect(await until { store.connection == .online })
    try await Task.sleep(for: .seconds(1))
    #expect(store.connection == .online)
}

@MainActor @Test func initialUnreachableEventuallyShowsOffline() async throws {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let fake = FakeRemote(.failed("Can't reach"))
    let store = BotStore(computer: randomComputer("Mac"), storage: storage) { _ in fake }
    defer { store.retire() }
    store.setActive(true)
    try await Task.sleep(for: .milliseconds(200))
    #expect(store.connection == .connecting)
    #expect(await until { store.connection == .offline("Can't reach") })
    #expect(store.lastError == nil)
}

@MainActor @Test func failedAutomaticReadDoesNotInterruptChatAndRecoversWithTheLink() async throws {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let fake = FakeRemote(.ready(.direct))
    let store = BotStore(computer: randomComputer("Mac"), storage: storage) { _ in fake }
    defer { store.retire() }
    store.setActive(true)
    #expect(await until { await fake.subscribed })
    await fake.emit(botEvent("b1", name: "Bot", rev: 1))
    #expect(await until { store.connection == .online })
    await fake.setReadFailure(true)
    store.setReading(UUID(), botId: "b1", active: true)
    #expect(await until { await fake.readAttempts == 1 })
    try await Task.sleep(for: .milliseconds(50))
    #expect(store.lastError == nil)
    await fake.set(.hostOffline(lastSeen: nil))
    #expect(await until { store.hostRoute == nil })
    #expect(store.connection == .online)
    #expect(store.canQueue)
    await fake.setReadFailure(false)
    await fake.set(.ready(.relay))
    #expect(await until { await fake.eventSubscriptionCount == 2 })
    await fake.emit(botEvent("b2", name: "Other bot", rev: 2))
    #expect(await until { await fake.readReceipts.count == 1 })
    #expect(store.connection == .online)
    #expect(store.lastError == nil)
}

@MainActor @Test func actionDuringReconnectWaitsInsteadOfFailing() async throws {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let fake = FakeRemote(.ready(.direct))
    let store = BotStore(computer: randomComputer("Mac"), storage: storage) { _ in fake }
    defer { store.retire() }
    store.setActive(true)
    #expect(await until { await fake.subscribed })
    await fake.emit(botEvent("b1", name: "Bot", rev: 1))
    #expect(await until { store.connection == .online })
    // A drop still inside its grace: the header says connected, the link is gone.
    await fake.set(.connecting)
    #expect(await until { store.hostRoute == nil })
    store.stop("b1")
    #expect(await until { store.shownConnection == .connecting })
    #expect(await !fake.calls.contains("stop"))
    await fake.set(.ready(.relay))
    #expect(await until { await fake.eventSubscriptionCount == 2 })
    await fake.emit(botEvent("b1", name: "Bot", rev: 2))
    #expect(await until { await fake.calls.contains("stop") })
    #expect(store.shownConnection == .online)
    #expect(store.lastError == nil)
}

@MainActor @Test func droppedCallIsRetriedOnlyWhenSafeToRepeat() async throws {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let fake = FakeRemote(.ready(.direct))
    let store = BotStore(computer: randomComputer("Mac"), storage: storage) { _ in fake }
    defer { store.retire() }
    store.setActive(true)
    #expect(await until { await fake.subscribed })
    await fake.emit(botEvent("b1", name: "Bot", rev: 1))
    #expect(await until { store.connection == .online })
    await fake.fail("stop", times: 1)
    store.stop("b1")
    #expect(await until { await fake.calls.filter { $0 == "stop" }.count == 2 })
    #expect(store.lastError == nil)
    // A call's opening can't be told apart from a second one, so it isn't sent twice.
    store.bots["b1"]?.lead = true
    await fake.fail("callStarted", times: 1)
    store.callStarted("b1", call: "c1")
    #expect(await until { store.lastError != nil })
    #expect(await fake.calls.filter { $0 == "callStarted" }.count == 1)
}

@MainActor @Test(.timeLimit(.minutes(1))) func offlineTapReconnectsBeforeGivingUp() async throws {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    // The first link can't reach the computer; a fresh attempt would.
    let down = FakeRemote(.failed("Can't reach"))
    let up = FakeRemote(.ready(.relay))
    var made = 0
    let store = BotStore(computer: randomComputer("Mac"), storage: storage) { _ in
        made += 1
        return made == 1 ? down : up
    }
    defer { store.retire() }
    store.setActive(true)
    let offline = await waitForObservedCondition { store.connection == .offline("Can't reach") }
    try #require(offline)
    store.stop("b1")
    let subscriptions = await up.checkpoints()
    let subscribed = await waitForTestEvent(subscriptions, matching: { $0 == .eventsSubscribed })
    try #require(subscribed)
    await up.emit(botEvent("b1", name: "Bot", rev: 1))
    let calls = await up.checkpoints()
    let stopped = await waitForTestEvent(calls, matching: { $0 == .called("stop") })
    try #require(stopped)
    let downCalls = await down.calls
    #expect(made == 2)
    #expect(downCalls.isEmpty)
    #expect(store.lastError == nil)
}

@MainActor @Test(.timeLimit(.minutes(1))) func offlineActionWaitsForFreshTransportBeforeSending() async throws {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let down = FakeRemote(.failed("Can't reach"))
    let up = FakeRemote(.ready(.relay))
    let requested = AsyncStream<Bool>.makeStream()
    let release = AsyncStream<Void>.makeStream()
    defer { release.continuation.finish(); requested.continuation.finish() }
    var made = 0
    let store = BotStore(computer: randomComputer("Mac"), storage: storage) { _ in
        made += 1
        if made == 1 { return down }
        requested.continuation.yield(true)
        for await _ in release.stream { break }
        try Task.checkCancellation()
        return up
    }
    defer { store.retire() }
    store.setActive(true)
    let offline = await waitForObservedCondition { store.connection == .offline("Can't reach") }
    try #require(offline)
    store.stop("b1")
    let preparing = await waitForTestEvent(requested.stream, matching: { $0 })
    try #require(preparing)
    let waiting = await waitForObservedCondition { store.waiting == 1 }
    try #require(waiting)
    let downCalls = await down.calls
    let before = await up.calls
    #expect(store.shownConnection == .connecting)
    #expect(downCalls.isEmpty)
    #expect(before.isEmpty)
    #expect(store.lastError == nil)

    release.continuation.yield(())
    let subscriptions = await up.checkpoints()
    let subscribed = await waitForTestEvent(subscriptions, matching: { $0 == .eventsSubscribed })
    try #require(subscribed)
    await up.emit(botEvent("b1", name: "Bot", rev: 1))
    let calls = await up.checkpoints()
    let stopped = await waitForTestEvent(calls, matching: { $0 == .called("stop") })
    try #require(stopped)
    let after = await up.calls
    #expect(after.filter { $0 == "stop" }.count == 1)
    #expect(made == 2)
    #expect(store.lastError == nil)
}

@MainActor @Test func permissionAnswerShowsOnTheCardAndIsSentOnce() async throws {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let fake = FakeRemote(.ready(.direct))
    let store = BotStore(computer: randomComputer("Mac"), storage: storage) { _ in fake }
    defer { store.retire() }
    store.setActive(true)
    #expect(await until { await fake.subscribed })
    await fake.emit(botEvent("b1", name: "Bot", rev: 1))
    #expect(await until { store.connection == .online })
    let card = Entry(id: "p1", seq: 1, botId: "b1", rev: 1, kind: "permission", turn: 1,
                     data: EntryData(status: "pending"), createdAt: 1, updatedAt: 1)
    store.respond(card, option: "allow")
    store.respond(card, option: "deny")
    #expect(store.answering["p1"] == "allow")
    #expect(await until { await fake.calls.contains("respondPermission") })
    try await Task.sleep(for: .milliseconds(100))
    #expect(await fake.calls.filter { $0 == "respondPermission" }.count == 1)
}

/// A catch-up sized like a real mirror: thirty chats, each with days of messages.
private func populate(_ fake: FakeRemote, bots: Int = 30, turnsPerBot: Int = 4) async -> Int64 {
    var rev: Int64 = 0
    for b in 0..<bots {
        rev += 1
        await fake.emit(botEvent("b\(b)", name: "Bot \(b)", rev: Int(rev)))
        for t in 0..<turnsPerBot {
            rev += 1
            // Days apart, so the chat reads as more than one sitting's.
            let day: Int64 = 1_700_000_000_000 + Int64(t) * 86_400_000
            await fake.emit(#"{"type":"entry","entry":{"id":"e-b\#(b)-\#(t)","seq":\#(t + 1),"botId":"b\#(b)","rev":\#(rev),"kind":"user","turn":1,"data":{"text":"m\#(t)"},"createdAt":\#(day),"updatedAt":\#(day)}}"#)
        }
    }
    return rev
}

@MainActor @Test func aHostUpdateKeepsTheCursorAndCatchesUpFromIt() async throws {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let fake = FakeRemote(.ready(.relay))
    await fake.setHello(helloJSON(version: "2.41.0"))
    let store = BotStore(computer: randomComputer("Mac"), storage: storage) { _ in fake }
    defer { store.retire() }
    store.setActive(true)
    #expect(await until { await fake.subscribed })
    // The stream's hello leads the catch-up, its `rev` the host's head (30 chats × 5 events).
    await fake.emit(#"{"type":"hello","hostId":"h1","rev":150}"#)
    let head = await populate(fake)
    #expect(await until { store.bots.count == 30 && store.chat("b29").count == 4 })
    // A chat paged to its start keeps that mark: nothing about the host updating resets it.
    store.historyComplete.insert("b0")

    // The host updates in place: its version changes, the link drops, and the phone reconnects.
    await fake.setHello(helloJSON(version: "2.41.1"))
    await fake.set(.connecting)
    await fake.set(.ready(.relay))
    #expect(await until { await fake.eventSubscriptionCount == 2 })
    // It asks from its rev, not from 0: the catch-up is only what is newer than every chat it holds.
    #expect(await fake.eventSinces == [0, head])
    let cursor = await fake.eventSinces.last!
    // The updated host's hello leads again; only the message written while the phone was away follows.
    await fake.emit(#"{"type":"hello","hostId":"h1","rev":\#(cursor + 1)}"#)
    await fake.emit(#"{"type":"entry","entry":{"id":"e-b29-4","seq":5,"botId":"b29","rev":\#(cursor + 1),"kind":"user","turn":1,"data":{"text":"while away"},"createdAt":1,"updatedAt":1}}"#)
    #expect(await until { store.chat("b29").last?.data.text == "while away" })
    // Multiday history stays as it was; paging back is untouched.
    #expect(store.chat("b0").map(\.id) == (0..<4).map { "e-b0-\($0)" })
    #expect(store.historyComplete.contains("b0"))
    #expect(store.chat("b29").map(\.seq) == [1, 2, 3, 4, 5])
}

/// The `since` of the first catch-up a store asks for over a mirror file holding `stamp` (as an older app wrote
/// it) or `format` and `schema`, with the host (store schema 8) updated meanwhile.
@MainActor private func firstSince(overCache field: String) async throws -> [Int64] {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let computer = randomComputer("Mac")
    let cacheURL = URL.cachesDirectory.appending(path:
        "codync-mirror-\(Bundle.main.bundleIdentifier ?? "app")-\(storage.id)-\(computer.id).json")
    defer { try? FileManager.default.removeItem(at: cacheURL) }
    let bots = (0..<30).map { #"{"id":"b\#($0)","name":"Bot \#($0)","rev":\#($0 + 1)}"# }.joined(separator: ",")
    let day: Int64 = 1_700_000_000_000
    let entries = (0..<30).flatMap { b in (0..<4).map { t in
        #"{"id":"e-b\#(b)-\#(t)","seq":\#(t + 1),"botId":"b\#(b)","rev":\#(100 + b * 4 + t),"kind":"user","turn":1,"data":{"text":"m\#(t)"},"createdAt":\#(day),"updatedAt":\#(day)}"#
    } }.joined(separator: ",")
    let cache = #"{\#(field),"hostId":"h1","rev":219,"bots":[\#(bots)],"entries":[\#(entries)]}"#
    try Data(cache.utf8).write(to: cacheURL, options: .atomic)

    let fake = FakeRemote(.ready(.relay))
    await fake.setHello(helloJSON(version: "2.41.0", schema: 8))
    let store = BotStore(computer: computer, storage: storage) { _ in fake }
    defer { store.retire() }
    #expect(store.chat("b0").map(\.id) == (0..<4).map { "e-b0-\($0)" })
    store.setActive(true)
    #expect(await until { await fake.subscribed })
    return await fake.eventSinces
}

@MainActor @Test func aCacheAnOlderBuildWroteKeepsItsCursorAcrossAppAndHostUpdates() async throws {
    // Another app build (Xcode Cloud numbers each one) for the host before its update.
    #expect(try await firstSince(overCache: #""stamp":"412/2.40.0""#) == [219])
    #expect(try await firstSince(overCache: #""format":\#(BotStore.mirrorFormat),"schema":8"#) == [219])
    // An older build saved partway through a refetch, or before its first hello: the stamp lacks the host.
    #expect(try await firstSince(overCache: #""stamp":"""#) == [0])
    #expect(try await firstSince(overCache: #""stamp":"412/""#) == [0])
    #expect(try await firstSince(overCache: #""stamp":"413""#) == [0])
    // Only another mirror format or store schema (rows rewritten in place, no new rev) fetches everything again.
    #expect(try await firstSince(overCache: #""format":\#(BotStore.mirrorFormat + 1)"#) == [0])
    #expect(try await firstSince(overCache: #""format":\#(BotStore.mirrorFormat),"schema":7"#) == [0])
}
