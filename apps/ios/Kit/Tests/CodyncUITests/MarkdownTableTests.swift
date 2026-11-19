import CoreGraphics
import Testing
@testable import CodyncUI

/// A table's columns take their widest cell when they fit the bubble, and share the bubble when
/// they don't, so a long cell wraps instead of running past it.
@Test func tableColumnsFitOrShareTheBubble() {
    #expect(TableLayout.columnWidths(ideal: [40, 60], available: 200) == [40, 60])
    let shared = TableLayout.columnWidths(ideal: [40, 600], available: 200)
    #expect(shared[0] == 40)
    #expect(shared[1] < 600)
    #expect(shared.reduce(0, +) <= 200)
    // A token with no space to break at gets less than its width, so Text wraps it mid-word.
    let token = TableLayout.columnWidths(ideal: [30, 900], available: 120)
    #expect(token[1] < 900)
    #expect(token.reduce(0, +) <= 120)
}
