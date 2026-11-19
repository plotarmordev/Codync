@testable import CodyncUI
import SwiftUI
import Testing
import WebKit

/// The file viewer's WebM player in the running app: WebKit decodes video only with the app's window and media
/// stack, which the hostless package tests lack.
@MainActor
struct WebMPlayerTests {
    @Test(arguments: ["vp8", "vp9"])
    func playsWebM(codec: String) async throws {
        let fixture = try #require(Bundle(for: Player.self).url(forResource: codec, withExtension: "webm"))
        let player = try Player(Data(contentsOf: fixture))
        #expect(await eventually { await player.currentTime() > 0 })
        #expect(!player.failed)
    }

    @Test func reportsWebMItCannotDecode() async throws {
        let player = try Player(Data([0x1A, 0x45, 0xDF, 0xA3]) + Data(repeating: 0x42, count: 512))
        #expect(await eventually { player.failed })
    }
}

/// `WebMPlayer` on screen in its own window, playing `data` saved as `FileViewer` saves a shared video.
@MainActor private final class Player {
    private(set) var failed = false
    private let window: UIWindow

    init(_ data: Data) throws {
        let url = URL.temporaryDirectory.appending(path: "codync-video-\(UUID().uuidString).webm")
        try data.write(to: url)
        window = UIWindow(windowScene: try #require(UIApplication.shared.connectedScenes.first as? UIWindowScene))
        window.rootViewController = UIHostingController(rootView: WebMPlayer(url: url) { [weak self] in self?.failed = true })
        window.makeKeyAndVisible()
    }

    func currentTime() async -> Double {
        guard let webView = Self.webView(in: window) else { return 0 }
        return (try? await webView.evaluateJavaScript("document.querySelector('video').currentTime") as? Double) ?? 0
    }

    private static func webView(in view: UIView) -> WKWebView? {
        view as? WKWebView ?? view.subviews.lazy.compactMap(webView).first
    }
}

/// Whether `condition` holds within 10 s.
@MainActor private func eventually(_ condition: @MainActor () async -> Bool) async -> Bool {
    for _ in 0..<100 {
        if await condition() { return true }
        try? await Task.sleep(for: .milliseconds(100))
    }
    return false
}
