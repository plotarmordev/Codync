import CodyncKit
import CodyncUI
import SwiftUI

/// Cold launch: the app-icon bot pops out of the system launch screen and looks around while the
/// computers connect and catch up, then zooms away onto a list that's already current.
/// Held at least `shortest` so the motion reads, never past `longest` (the cached list is behind it); `done` once it
/// has faded out, so nothing under it counts as shown before then.
struct LaunchSplash: View {
    let stores: [BotStore]
    let done: () -> Void
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var shown = false
    @State private var waited = false
    @State private var leaving = false

    private static let shortest: Duration = .milliseconds(700)
    private static let longest: Duration = .milliseconds(1800)

    private var ready: Bool { waited && stores.allSatisfy(\.caughtUp) }

    var body: some View {
        TimelineView(.animation(paused: reduceMotion)) { context in
            let t = context.date.timeIntervalSinceReferenceDate
            CharacterAvatar(shape: "squircle", tint: Palette.text, size: 112, mood: .working)
                .offset(y: reduceMotion ? 0 : sin(t * 2.4) * 4)
        }
        .scaleEffect(shown || reduceMotion ? 1 : 0.6)
        .opacity(shown ? 1 : 0)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Palette.background)
        // Leaving: the bot grows a little as everything fades through to the app.
        .scaleEffect(leaving && !reduceMotion ? 1.15 : 1)
        .opacity(leaving ? 0 : 1)
        .allowsHitTesting(!leaving)
        .accessibilityLabel("Connecting")
        .task {
            withAnimation(.spring(duration: 0.5, bounce: 0.35)) { shown = true }
            try? await Task.sleep(for: Self.shortest)
            waited = true
            try? await Task.sleep(for: Self.longest - Self.shortest)
            leave()
        }
        .onChange(of: ready, initial: true) { _, ready in if ready { leave() } }
    }

    private func leave() {
        guard !leaving else { return }
        withAnimation(.easeOut(duration: 0.35)) { leaving = true } completion: { done() }
    }
}

/// The launch splash over the app while `app.launching`; screens under it read `launchCovered`.
private struct LaunchCover: ViewModifier {
    let app: AppStore
    let stores: [BotStore]

    func body(content: Content) -> some View {
        content
            .environment(\.launchCovered, app.launching)
            .overlay { if app.launching { LaunchSplash(stores: stores) { app.launching = false } } }
    }
}

extension View {
    func launchSplash(_ app: AppStore, stores: [BotStore]) -> some View {
        modifier(LaunchCover(app: app, stores: stores))
    }
}
