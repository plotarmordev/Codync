import Foundation

/// An immutable host snapshot published by a bot. Separate from user uploads.
public struct SharedFile: Codable, Hashable, Sendable, Identifiable {
    public let id: String
    public let name: String
    public let size: Int64
    public let sha256: String
    /// A sound the chat plays in place, as the computer stamps it; missing (an older computer) means a plain file card.
    public let playsInChat: Bool

    public init(id: String, name: String, size: Int64, sha256: String, playsInChat: Bool = false) {
        self.id = id
        self.name = name
        self.size = size
        self.sha256 = sha256
        self.playsInChat = playsInChat
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        self.init(id: try c.decode(String.self, forKey: .id), name: try c.decode(String.self, forKey: .name),
                  size: try c.decode(Int64.self, forKey: .size), sha256: try c.decode(String.self, forKey: .sha256),
                  playsInChat: try c.decodeIfPresent(Bool.self, forKey: .playsInChat) ?? false)
    }

    /// The largest shared image the phone fetches whole to show.
    private static let inlineBytes: Int64 = 20 * 1024 * 1024
    /// An image small enough to show in the chat.
    public var showsAsImage: Bool { Attachment.isImage(name) && size <= Self.inlineBytes }

    /// A kind of file the phone opens in place: Markdown, text/code, a PDF or a video. Images and sounds
    /// keep their own previews, so they never get one here.
    public enum FilePreview: Sendable {
        case markdown, text, pdf, video

        /// The largest file the viewer fetches; a bigger one still opens, with a notice instead.
        public var maxBytes: Int64 {
            switch self {
            case .markdown, .text: 10 * 1024 * 1024
            case .pdf, .video: 25 * 1024 * 1024
            }
        }
    }

    /// A readable file's viewer kind, by extension like `Attachment.isImage`; nil for anything else.
    public var preview: FilePreview? {
        switch (name as NSString).pathExtension.lowercased() {
        case "md", "markdown": .markdown
        case "txt", "log", "csv", "tsv", "json", "jsonl", "xml", "yaml", "yml", "toml", "ini",
             "swift", "kt", "kts", "java", "js", "jsx", "ts", "tsx", "py", "rb", "go", "rs",
             "c", "h", "cpp", "hpp", "cs", "sh", "sql", "html", "css": .text
        case "pdf": .pdf
        case "mp4", "mov", "m4v", "webm": .video
        default: nil
        }
    }

    /// Small enough to open inside the app.
    public var opensInline: Bool { preview.map { size <= $0.maxBytes } ?? false }

    /// A video's container, read from its bytes whatever the file is called. They pick the player: AVFoundation plays
    /// ISO media but not WebM, which WebKit plays.
    public enum VideoContainer: Sendable {
        /// MP4, MOV or M4V: an ISO media file's `ftyp` box or an older QuickTime file's first `moov`/`mdat`/`wide` atom.
        case isoMedia
        /// Matroska/WebM's EBML header.
        case webM

        public init?(_ data: Data) {
            let head = [UInt8](data.prefix(8))
            guard head.count == 8 else { return nil }
            if head.starts(with: [0x1A, 0x45, 0xDF, 0xA3]) {
                self = .webM
            } else if ["ftyp", "moov", "mdat", "wide"].contains(String(decoding: head[4...], as: UTF8.self)) {
                self = .isoMedia
            } else {
                return nil
            }
        }
    }

    public func validate() throws {
        guard size >= 0, size <= 100 * 1024 * 1024,
              sha256.count == 64, sha256.allSatisfy({ "0123456789abcdef".contains($0) }),
              !name.isEmpty, name.utf8.count <= 200, name != ".", name != "..",
              !name.contains("/"), !name.contains("\\"),
              !name.unicodeScalars.contains(where: { CharacterSet.controlCharacters.contains($0) }) else {
            throw FileDownloadError.invalidMetadata
        }
    }
}
