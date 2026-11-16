import Foundation
import Observation

/// Milliseconds from each cold or foreground open to its connection and rendering milestones.
@Observable
@MainActor
public final class OpenTiming {
    public static let shared = OpenTiming()
    public private(set) var opens = 0

    private let now: () -> ContinuousClock.Instant
    private let capture: (AnalyticsEvent) -> Void
    private var openedAt: ContinuousClock.Instant?
    private var kind = AnalyticsEvent.OpenKind.cold
    private var backgrounded = false
    private var computers = 0
    private var sent: Set<Key> = []

    private struct Key: Hashable {
        let computer: ComputerID
        let phase: AnalyticsEvent.OpenPhase
    }

    init(now: @escaping () -> ContinuousClock.Instant = { .now },
         capture: @escaping (AnalyticsEvent) -> Void = { Analytics.shared.capture($0) }) {
        self.now = now
        self.capture = capture
    }

    /// Starts before the saved computers open their streams during app initialization.
    public func beginCold() {
        opens += 1
        openedAt = now()
        kind = .cold
        sent = []
    }

    public func setComputerCount(_ count: Int) {
        computers = count
    }

    public func becameActive(computers: Int) {
        self.computers = computers
        if backgrounded {
            opens += 1
            openedAt = now()
            kind = .foreground
            sent = []
        }
        backgrounded = false
    }

    public func enteredBackground() {
        backgrounded = true
    }

    public func hasRecorded(_ phase: AnalyticsEvent.OpenPhase, computer: ComputerID) -> Bool {
        sent.contains(Key(computer: computer, phase: phase))
    }

    public func record(_ phase: AnalyticsEvent.OpenPhase, computer: ComputerID,
                       chats: Int? = nil, messages: Int? = nil) {
        guard let openedAt, sent.insert(Key(computer: computer, phase: phase)).inserted else { return }
        let ms = Int((now() - openedAt) / .milliseconds(1))
        capture(.openTiming(phase, open: kind, ms: ms, computer: computer,
                            computers: computers, chats: chats, messages: messages))
    }
}
