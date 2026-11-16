import Foundation
import Testing
@testable import CodyncKit

@Test func openTimingUsesHostDiagnosticWireShape() throws {
    let encoder = JSONEncoder()
    encoder.outputFormatting = .sortedKeys
    let data = try encoder.encode(PhonePerformance(phase: .catchUp, open: .foreground, ms: 1250, chats: 8, messages: 31))
    #expect(String(decoding: data, as: UTF8.self)
        == #"{"chats":8,"messages":31,"ms":1250,"open":"foreground","phase":"catch_up","platform":"ios","type":"open_timing"}"#)
}
