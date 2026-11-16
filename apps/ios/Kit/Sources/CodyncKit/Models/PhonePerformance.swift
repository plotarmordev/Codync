import Foundation

/// A phone's open-timing measurement for its connected host's local diagnostic log.
/// Dedupe keys stay on the phone: no computer, bot or message identity is sent.
public struct PhonePerformance: Encodable, Sendable {
    public let phase: OpenPhase
    public let open: OpenKind
    public let ms: Int
    public let chats: Int?
    public let messages: Int?

    public init(phase: OpenPhase, open: OpenKind, ms: Int, chats: Int? = nil, messages: Int? = nil) {
        self.phase = phase
        self.open = open
        self.ms = ms
        self.chats = chats
        self.messages = messages
    }

    private enum CodingKeys: String, CodingKey {
        case type, platform, phase, open, ms, chats, messages
    }

    public func encode(to encoder: any Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode("open_timing", forKey: .type)
        try container.encode("ios", forKey: .platform)
        try container.encode(phase.rawValue, forKey: .phase)
        try container.encode(open.rawValue, forKey: .open)
        try container.encode(ms, forKey: .ms)
        try container.encodeIfPresent(chats, forKey: .chats)
        try container.encodeIfPresent(messages, forKey: .messages)
    }
}
