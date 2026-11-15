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

@MainActor @Test func aReplyArrivingWhileFollowingKeepsALongChatPinnedToTheEnd() async throws {
    var following = true
    let binding = Binding(get: { following }, set: { following = $0 })
    let (controller, window) = hostConversation(rows: longChatRows(count: 40), following: binding)
    defer { window.isHidden = true }
    try await Task.sleep(for: .milliseconds(200))
    let list = try #require(conversationList(in: controller.view))

    controller.rootView = ConversationList(rows: longChatRows(count: 41), following: binding)
    try await Task.sleep(for: .milliseconds(200))

    #expect(list.numberOfItems(inSection: 0) == 41)
    #expect(abs(list.contentOffset.y - list.endOffset) < 1)
    #expect(!list.scrollingToEnd)
    #expect(list.following && following)
}

@MainActor @Test func earlierRowsArrivingWhileScrolledAwayKeepTheVisibleMessageInPlace() async throws {
    var following = false
    let binding = Binding(get: { following }, set: { following = $0 })
    let (controller, window) = hostConversation(rows: longChatRows(count: 40), following: binding)
    defer { window.isHidden = true }
    try await Task.sleep(for: .milliseconds(200))
    let list = try #require(conversationList(in: controller.view))
    list.contentOffset.y = 2_000
    list.layoutIfNeeded()
    let visible = try #require(list.indexPathsForVisibleItems.max())
    let attributes = try #require(list.layoutAttributesForItem(at: visible))
    let relativeY = attributes.frame.minY - list.contentOffset.y

    controller.rootView = ConversationList(rows: longChatRows(count: 20, prefix: "old") + longChatRows(count: 40),
                                           following: binding)
    try await Task.sleep(for: .milliseconds(200))

    let newVisible = try #require(list.indexPathsForVisibleItems.max())
    let newAttributes = try #require(list.layoutAttributesForItem(at: newVisible))
    #expect(list.numberOfItems(inSection: 0) == 60)
    #expect(newVisible.item == visible.item + 20)
    #expect(abs(newAttributes.frame.minY - list.contentOffset.y - relativeY) < 1)
    #expect(!list.following && !following)
}

@MainActor @Test func sendingWhileScrolledAwayReturnsToTheLatestMessageAndSettles() async throws {
    var following = false
    let binding = Binding(get: { following }, set: { following = $0 })
    let (controller, window) = hostConversation(rows: longChatRows(count: 40), following: binding)
    defer { window.isHidden = true }
    try await Task.sleep(for: .milliseconds(200))
    let list = try #require(conversationList(in: controller.view))
    list.contentOffset.y = 1_200

    let sent = ConversationRow("sent", isUserMessage: true, value: 99) { Color.clear.frame(height: 120) }
    controller.rootView = ConversationList(rows: longChatRows(count: 40) + [sent], following: binding)
    try await Task.sleep(for: .milliseconds(50))
    var offsets: [CGFloat] = []
    for _ in 0..<100 {
        offsets.append(list.contentOffset.y)
        if !list.scrollingToEnd && abs(list.contentOffset.y - list.endOffset) < 1 { break }
        try await Task.sleep(for: .milliseconds(20))
    }

    #expect(offsets.allSatisfy { $0 <= list.endOffset + 1 })
    #expect(!list.scrollingToEnd)
    #expect(abs(list.contentOffset.y - list.endOffset) < 1)
    #expect(list.following && following)
}

private func longChatRows(count: Int, prefix: String = "row") -> [ConversationRow] {
    (0..<count).map { index in
        ConversationRow("\(prefix)-\(index)", value: index) { Color.clear.frame(height: 120) }
    }
}

@MainActor private func hostConversation(rows: [ConversationRow], following: Binding<Bool>)
    -> (UIHostingController<ConversationList>, UIWindow) {
    let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 390, height: 844))
    let controller = UIHostingController(rootView: ConversationList(rows: rows, following: following))
    window.rootViewController = controller
    window.makeKeyAndVisible()
    return (controller, window)
}

@MainActor private func conversationList(in view: UIView) -> ConversationCollectionView? {
    if let list = view as? ConversationCollectionView { return list }
    return view.subviews.lazy.compactMap { conversationList(in: $0) }.first
}
