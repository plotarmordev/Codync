@testable import Codync
@testable import CodyncKit
@testable import CodyncUI
import SwiftUI
import Testing

/// The real roster and transcript in the running app's window: their rendering, not direct milestone calls,
/// finishes each open's timing sequence.
@MainActor
struct OpenTimingFlowTests {
    @Test func coldAndForegroundOpensRecordAfterTheActualScreensRender() async throws {
        let (storage, suite) = context()
        defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
        var clock = ContinuousClock.now
        var events: [AnalyticsEvent] = []
        let timing = OpenTiming(now: {
            clock += .milliseconds(1)
            return clock
        }, capture: { events.append($0) })
        timing.beginCold()
        timing.becameActive()

        let computer = randomComputer("Mac")
        storage.computers = [computer]
        let remote = FakeRemote(.ready(.direct))
        let accounts = AccountStore(storage: storage, cloud: nil, openTiming: timing) { computer in
            let store = BotStore(computer: computer, storage: storage) { _ in remote }
            store.openTiming = timing
            return store
        }
        let store = try #require(accounts.store(for: computer.id))
        let app = AppStore.shared
        app.launching = false
        defer { accounts.retire() }

        let window = UIWindow(windowScene: try #require(UIApplication.shared.connectedScenes.first as? UIWindowScene))
        window.frame = UIScreen.main.bounds
        window.rootViewController = UIHostingController(rootView: BotListView().environment(app).environment(accounts).environment(app.account))
        window.makeKeyAndVisible()
        defer { window.isHidden = true }
        #expect(await until { await remote.subscribed })
        await remote.emit(#"{"type":"hello","hostId":"h1","rev":3,"catchUp":3}"#)
        await remote.emit(#"{"type":"bot","bot":{"id":"b1","name":"Bot","rev":1,"lastAt":2,"lastMessage":"new"}}"#)
        await remote.emit(#"{"type":"entry","entry":{"id":"e1","seq":1,"botId":"b1","rev":2,"kind":"agent","turn":1,"data":{"text":"new","final":true},"createdAt":1,"updatedAt":1}}"#)
        // Created after b1's message: the newest bot, but with nothing to show.
        await remote.emit(#"{"type":"bot","bot":{"id":"b0","name":"New","rev":3,"lastAt":3}}"#)
        #expect(await until { store.roster.count == 2 })
        #expect(!app.launching)
        #expect(await until { events.count == 4 })

        #expect(store.chat("b1").map(\.id) == ["e1"])
        #expect(ChatItem.build(store.chat("b1")).last?.id == "e1")
        window.rootViewController = UIHostingController(rootView: ThreadView(botId: "b1").environment(store))
        window.makeKeyAndVisible()
        #expect(await until { events.count == 5 })
        #expect(phases(events) == [.linkReady, .hello, .catchUp, .chatList, .openChat])
        #expect(milliseconds(events) == [1, 2, 3, 4, 5])

        // The visible transcript survives this return, but must wait for the new subscription's catch-up.
        await remote.fail("hello", times: 1)
        accounts.setActive(false)
        timing.enteredBackground()
        timing.becameActive()
        accounts.setActive(true)
        #expect(await until { events.count == 6 })
        #expect(phases(events) == [.linkReady, .hello, .catchUp, .chatList, .openChat, .linkReady])
        #expect(await until { await remote.subscribed })
        await remote.emit(#"{"type":"hello","hostId":"h1","rev":2,"catchUp":1}"#)
        await remote.emit(#"{"type":"bot","bot":{"id":"b1","name":"Bot","rev":1,"lastAt":2,"lastMessage":"new"}}"#)
        #expect(await until { events.count == 9 })
        #expect(phases(events).suffix(4) == [.linkReady, .hello, .catchUp, .openChat])

        window.rootViewController = UIHostingController(rootView: BotListView().environment(app).environment(accounts).environment(app.account))
        #expect(await until { events.count == 10 })
        #expect(phases(events).last == .chatList)
        #expect(milliseconds(events).suffix(5) == [1, 2, 3, 4, 5])
    }

    @Test func aChatOpenedUnderTheLaunchSplashRecordsOnceItHasFadedOut() async throws {
        let (storage, suite) = context()
        defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
        var events: [AnalyticsEvent] = []
        let timing = OpenTiming(capture: { events.append($0) })
        timing.beginCold()
        timing.becameActive()

        let remote = FakeRemote(.ready(.direct))
        let store = BotStore(computer: randomComputer("Mac"), storage: storage) { _ in remote }
        store.openTiming = timing
        store.setActive(true)
        defer { store.retire() }
        let app = AppStore.shared
        app.launching = true
        defer { app.launching = false }

        // A cold deep link: the chat is pushed under the app's real splash, which leaves after its catch-up.
        let window = UIWindow(windowScene: try #require(UIApplication.shared.connectedScenes.first as? UIWindowScene))
        window.frame = UIScreen.main.bounds
        window.rootViewController = UIHostingController(rootView: ThreadView(botId: "b1").environment(store)
            .launchSplash(app, stores: [store]))
        window.makeKeyAndVisible()
        defer { window.isHidden = true }
        #expect(await until { await remote.subscribed })
        await remote.emit(#"{"type":"hello","hostId":"h1","rev":2,"catchUp":2}"#)
        await remote.emit(#"{"type":"bot","bot":{"id":"b1","name":"Bot","rev":1,"lastAt":2,"lastMessage":"new"}}"#)
        await remote.emit(#"{"type":"entry","entry":{"id":"e1","seq":1,"botId":"b1","rev":2,"kind":"agent","turn":1,"data":{"text":"new","final":true},"createdAt":1,"updatedAt":1}}"#)
        #expect(await until { events.count == 4 })

        // The splash holds 700 ms from its first frame, then fades out over 350 ms; the transcript shows only after.
        #expect(phases(events).last == .openChat)
        #expect(try #require(milliseconds(events).last ?? nil) >= 1050)
    }

    private func phases(_ events: [AnalyticsEvent]) -> [AnalyticsEvent.OpenPhase?] {
        events.map { event in
            guard case let .openTiming(phase, _, _, _, _, _, _) = event else { return nil }
            return phase
        }
    }

    private func milliseconds(_ events: [AnalyticsEvent]) -> [Int?] {
        events.map { event in
            guard case let .openTiming(_, _, ms, _, _, _, _) = event else { return nil }
            return ms
        }
    }
}
