import Foundation
import Testing
@testable import CodyncKit
@testable import CodyncUI

@MainActor @Test func oneOpenRecordsEachTimingOnceInOrder() async throws {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    var clock = ContinuousClock.now
    var events: [AnalyticsEvent] = []
    let timing = OpenTiming(now: {
        clock += .milliseconds(1)
        return clock
    }, capture: { events.append($0) })
    timing.beginCold()
    timing.setComputerCount(1)
    timing.becameActive(computers: 1)

    let computer = randomComputer("Mac")
    let remote = FakeRemote(.ready(.direct))
    let store = BotStore(computer: computer, storage: storage) { _ in remote }
    store.openTiming = timing
    store.setActive(true)
    defer { store.retire() }
    #expect(await until { await remote.subscribed })
    await remote.emit(#"{"type":"hello","hostId":"h1","rev":2,"catchUp":2}"#)
    await remote.emit(botEvent("b1", name: "Bot", rev: 1))
    await remote.emit(#"{"type":"entry","entry":{"id":"e1","seq":1,"botId":"b1","rev":2,"kind":"agent","turn":1,"data":{"text":"new"},"createdAt":1,"updatedAt":1}}"#)
    #expect(await until { events.count == 3 })
    timing.record(.chatList, computer: computer.id)
    timing.record(.openChat, computer: computer.id)
    timing.record(.linkReady, computer: computer.id)
    timing.record(.hello, computer: computer.id)
    timing.record(.catchUp, computer: computer.id, chats: 1, messages: 1)
    timing.record(.chatList, computer: computer.id)
    timing.record(.openChat, computer: computer.id)

    #expect(events.count == 5)
    let phases = events.map { event -> AnalyticsEvent.OpenPhase? in
        guard case let .openTiming(phase, _, _, _, _, _, _) = event else { return nil }
        return phase
    }
    #expect(phases == [.linkReady, .hello, .catchUp, .chatList, .openChat])
    let milliseconds = events.map { event -> Int? in
        guard case let .openTiming(_, _, ms, _, _, _, _) = event else { return nil }
        return ms
    }
    #expect(milliseconds == [1, 2, 3, 4, 5])
    for event in events {
        guard case let .openTiming(_, open, _, id, computers, _, _) = event else {
            Issue.record("unexpected event")
            continue
        }
        #expect(open == .cold)
        #expect(id == computer.id)
        #expect(computers == 1)
    }
    guard case let .openTiming(_, _, _, _, _, chats, messages) = events[2] else { return }
    #expect(chats == 1)
    #expect(messages == 1)
}

@MainActor @Test func returningFromBackgroundStartsANewTimingOpen() {
    var clock = ContinuousClock.now
    var events: [AnalyticsEvent] = []
    let timing = OpenTiming(now: {
        clock += .milliseconds(1)
        return clock
    }, capture: { events.append($0) })
    timing.beginCold()
    timing.becameActive(computers: 1)
    timing.record(.linkReady, computer: "computer")
    timing.becameActive(computers: 2)
    #expect(events.count == 1)
    timing.enteredBackground()
    timing.becameActive(computers: 2)
    timing.record(.linkReady, computer: "computer")

    guard events.count == 2,
          case let .openTiming(_, firstOpen, firstMs, _, firstComputers, _, _) = events[0],
          case let .openTiming(_, secondOpen, secondMs, _, secondComputers, _, _) = events[1] else {
        Issue.record("missing open timings")
        return
    }
    #expect(firstOpen == .cold)
    #expect(secondOpen == .foreground)
    #expect(firstMs == 1)
    #expect(secondMs == 1)
    #expect(firstComputers == 1)
    #expect(secondComputers == 2)
}

@MainActor @Test func foregroundReturnWithVoiceCallRecordsAnAlreadyReadyLink() async throws {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    var events: [AnalyticsEvent] = []
    let timing = OpenTiming(capture: { events.append($0) })
    timing.beginCold()
    timing.becameActive(computers: 1)

    let computer = randomComputer("Mac")
    let remote = FakeRemote(.ready(.direct))
    let store = BotStore(computer: computer, storage: storage) { _ in remote }
    store.openTiming = timing
    store.setActive(true)
    defer { store.retire() }
    #expect(await until { await remote.subscribed })
    await remote.emit(#"{"type":"hello","hostId":"h1","rev":1,"catchUp":1}"#)
    await remote.emit(botEvent("b1", name: "Bot", rev: 1))
    #expect(await until { events.count == 3 })

    store.beginVoiceCall(UUID(), hear: { _ in }, end: {})
    store.setActive(false)
    timing.enteredBackground()
    timing.becameActive(computers: 1)
    store.setActive(true)

    #expect(await until { events.count == 6 })
    let phases = events.map { event -> AnalyticsEvent.OpenPhase? in
        guard case let .openTiming(phase, _, _, _, _, _, _) = event else { return nil }
        return phase
    }
    #expect(phases == [.linkReady, .hello, .catchUp, .linkReady, .hello, .catchUp])
    #expect(await remote.eventSinces.count == 1)
    guard case let .openTiming(_, open, _, _, _, chats, messages) = events[5] else { return }
    #expect(open == .foreground)
    #expect(chats == 1)
    #expect(messages == 0)
}

@MainActor @Test func foregroundReturnAfterARetainedLinkDroppedWaitsForReconnect() async throws {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    var events: [AnalyticsEvent] = []
    let timing = OpenTiming(capture: { events.append($0) })
    timing.beginCold()
    timing.becameActive(computers: 1)

    let remote = FakeRemote(.ready(.direct))
    let store = BotStore(computer: randomComputer("Mac"), storage: storage) { _ in remote }
    store.openTiming = timing
    store.setActive(true)
    defer { store.retire() }
    #expect(await until { await remote.subscribed })
    await remote.emit(#"{"type":"hello","hostId":"h1","rev":1,"catchUp":1}"#)
    await remote.emit(botEvent("b1", name: "Bot", rev: 1))
    #expect(await until { events.count == 3 })

    store.beginVoiceCall(UUID(), hear: { _ in }, end: {})
    store.setActive(false)
    await remote.set(.connecting)
    #expect(await until { store.hostRoute == nil })
    timing.enteredBackground()
    timing.becameActive(computers: 1)
    store.setActive(true)

    #expect(events.count == 3)
}

@MainActor @Test func catchUpCountsAChatRepresentedOnlyByAnEntry() async throws {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    var events: [AnalyticsEvent] = []
    let timing = OpenTiming(capture: { events.append($0) })
    timing.beginCold()
    timing.becameActive(computers: 1)
    let computer = randomComputer("Mac")
    let remote = FakeRemote(.ready(.direct))
    let store = BotStore(computer: computer, storage: storage) { _ in remote }
    store.openTiming = timing
    store.setActive(true)
    defer { store.retire() }
    #expect(await until { await remote.subscribed })
    await remote.emit(#"{"type":"hello","hostId":"h1","rev":1,"catchUp":1}"#)
    await remote.emit(#"{"type":"entry","entry":{"id":"e1","seq":1,"botId":"entry-only","rev":1,"kind":"agent","turn":1,"data":{"text":"new"},"createdAt":1,"updatedAt":1}}"#)

    #expect(await until { events.count == 3 })
    guard case let .openTiming(_, _, _, _, _, chats, messages) = events[2] else { return }
    #expect(chats == 1)
    #expect(messages == 1)
}

@MainActor @Test func foregroundReturnDuringCatchUpWaitsForTheCurrentSubscription() async throws {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    var events: [AnalyticsEvent] = []
    let timing = OpenTiming(capture: { events.append($0) })
    timing.beginCold()
    timing.becameActive(computers: 1)
    let remote = FakeRemote(.ready(.direct))
    let store = BotStore(computer: randomComputer("Mac"), storage: storage) { _ in remote }
    store.openTiming = timing
    store.setActive(true)
    defer { store.retire() }
    #expect(await until { await remote.subscribed })
    await remote.emit(#"{"type":"hello","hostId":"h1","rev":1,"catchUp":1}"#)
    await remote.emit(botEvent("b1", name: "Bot", rev: 1))
    #expect(await until { events.count == 3 })

    store.beginVoiceCall(UUID(), hear: { _ in }, end: {})
    store.setActive(false)
    await remote.failEvents()
    #expect(await until { await remote.eventSinces.count == 2 })
    await remote.emit(#"{"type":"hello","hostId":"h1","rev":2,"catchUp":1}"#)
    timing.enteredBackground()
    timing.becameActive(computers: 1)
    store.setActive(true)
    #expect(events.count == 5)

    await remote.emit(botEvent("b1", name: "Bot", rev: 2))
    #expect(await until { events.count == 6 })
    guard case let .openTiming(phase, open, _, _, _, chats, messages) = events[5] else { return }
    #expect(phase == .catchUp)
    #expect(open == .foreground)
    #expect(chats == 1)
    #expect(messages == 0)
}

@MainActor @Test func catchUpCountsExcludeARetryAndIncludeChatsWithoutEntries() async throws {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    var events: [AnalyticsEvent] = []
    let timing = OpenTiming(capture: { events.append($0) })
    timing.beginCold()
    timing.becameActive(computers: 1)

    let computer = randomComputer("Mac")
    let remote = FakeRemote(.ready(.direct))
    let store = BotStore(computer: computer, storage: storage) { _ in remote }
    store.openTiming = timing
    store.setActive(true)
    defer { store.retire() }
    #expect(await until { await remote.subscribed })
    await remote.emit(#"{"type":"hello","hostId":"h1","rev":2,"catchUp":3}"#)
    await remote.emit(botEvent("b1", name: "Bot", rev: 1))
    await remote.emit(#"{"type":"entry","entry":{"id":"e1","seq":1,"botId":"b1","rev":2,"kind":"agent","turn":1,"data":{"text":"new"},"createdAt":1,"updatedAt":1}}"#)
    #expect(await until { !store.chat("b1").isEmpty })

    await remote.failEvents()
    #expect(await until { await remote.eventSinces.count == 2 })
    await remote.emit(#"{"type":"hello","hostId":"h1","rev":1,"catchUp":1}"#)
    await remote.emit(botEvent("b1", name: "Bot", rev: 1))

    #expect(await until {
        events.contains { event in
            guard case let .openTiming(phase, _, _, _, _, _, _) = event else { return false }
            return phase == .catchUp
        }
    })
    let catchUp = try #require(events.first { event in
        guard case let .openTiming(phase, _, _, _, _, _, _) = event else { return false }
        return phase == .catchUp
    })
    guard case let .openTiming(_, _, _, _, _, chats, messages) = catchUp else { return }
    #expect(chats == 1)
    #expect(messages == 0)
}
