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
