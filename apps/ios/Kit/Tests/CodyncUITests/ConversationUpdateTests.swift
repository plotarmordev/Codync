import SwiftUI
import Testing
import UIKit
@testable import CodyncUI

@MainActor @Test func aStreamingReplyDoesNotRebuildUnchangedVisibleRows() async {
    var firstBuilds = 0
    var secondBuilds = 0
    var following = true
    let binding = Binding(get: { following }, set: { following = $0 })
    func rows(_ secondValue: Int) -> [ConversationRow] {
        [ConversationRow("first", value: 1) {
            let _ = firstBuilds += 1
            Text("First")
        }, ConversationRow("second", value: secondValue) {
            let _ = secondBuilds += 1
            Text("Second \(secondValue)")
        }]
    }

    let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 390, height: 844))
    let controller = UIHostingController(rootView: ConversationList(rows: rows(0), following: binding))
    window.rootViewController = controller
    window.makeKeyAndVisible()
    defer { window.isHidden = true }
    try? await Task.sleep(for: .milliseconds(100))
    let initialFirstBuilds = firstBuilds
    let initialSecondBuilds = secondBuilds

    #expect(initialFirstBuilds > 0)
    #expect(initialSecondBuilds > 0)

    controller.rootView = ConversationList(rows: rows(1), following: binding)
    try? await Task.sleep(for: .milliseconds(100))

    #expect(firstBuilds == initialFirstBuilds)
    #expect(secondBuilds > initialSecondBuilds)
}
