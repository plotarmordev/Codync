import CryptoKit
import Foundation
import Testing
@testable import CodyncKit

private struct FileTransport: HostTransport {
    let bytes: Data
    var truncate = false
    var wrongSize = false
    func call(_ method: String, body: Data, timeout: TimeInterval) async throws -> Data {
        #expect(method == "readFile")
        let request = try #require(JSONSerialization.jsonObject(with: body) as? [String: Any])
        let offset = try #require(request["offset"] as? Int)
        let end = min(offset + FileDownload.chunkSize, bytes.count)
        let data = truncate ? Data() : bytes.subdata(in: offset..<end)
        return try JSONSerialization.data(withJSONObject: ["data": data.base64EncodedString(), "size": bytes.count + (wrongSize ? 1 : 0)])
    }
    func stream(_ request: HostStreamRequest) -> AsyncThrowingStream<Data, Error> { AsyncThrowingStream { $0.finish() } }
    func states() -> AsyncStream<LinkState> { AsyncStream { $0.finish() } }
}

private func metadata(_ data: Data, name: String = "file.bin") -> SharedFile {
    SharedFile(id: UUID().uuidString, name: name, size: Int64(data.count), sha256: SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined())
}

@Test func generatedFilesDownloadExactBinaryAndEmptyBytes() async throws {
    let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    defer { try? FileManager.default.removeItem(at: directory) }
    let writer = FileDownload()
    for bytes in [Data(), Data((0..<(FileDownload.chunkSize + 7)).map { UInt8($0 % 256) })] {
        let file = metadata(bytes, name: ".download-partial")
        let url = try await writer.save(client: HostClient(transport: FileTransport(bytes: bytes)), entryId: "entry", file: file, directory: directory) { _ in }
        #expect(try Data(contentsOf: url) == bytes)
        #expect(url.lastPathComponent == ".download-partial")
        try await writer.remove(url)
    }
}

@Test func sharedFileReadsWholeInMemoryAndChecksItsDigest() async throws {
    let bytes = Data((0..<(FileDownload.chunkSize + 7)).map { UInt8($0 % 256) })
    let client = HostClient(transport: FileTransport(bytes: bytes))
    #expect(try await FileDownload().data(client: client, entryId: "entry", file: metadata(bytes, name: "picture.png")) == bytes)
    let bad = SharedFile(id: "id", name: "picture.png", size: Int64(bytes.count), sha256: String(repeating: "0", count: 64))
    await #expect(throws: FileDownloadError.self) { try await FileDownload().data(client: client, entryId: "entry", file: bad) }
}

@Test func sharedFilePreviewFollowsTheExtension() {
    let file = { SharedFile(id: "id", name: $0, size: 0, sha256: String(repeating: "0", count: 64)) }
    #expect(file("notes.md").preview == .markdown)
    #expect(file("notes.MARKDOWN").preview == .markdown)
    for name in ["a.txt", "data.csv", "x.json", "main.swift"] { #expect(file(name).preview == .text, "\(name)") }
    #expect(file("report.pdf").preview == .pdf)
    for name in ["clip.mp4", "clip.MOV", "clip.m4v", "clip.webm"] { #expect(file(name).preview == .video, "\(name)") }
    for name in ["archive.zip", "photo.png", "voice.m4a", "README"] { #expect(file(name).preview == nil, "\(name)") }
}

@Test func sharedFileOpensInlineUpToItsKindsLimit() {
    let file = { SharedFile(id: "id", name: $0, size: $1, sha256: String(repeating: "0", count: 64)) }
    let mb = Int64(1024 * 1024)
    for (name, limit) in [("notes.md", 10 * mb), ("data.csv", 10 * mb), ("x.json", 10 * mb), ("report.pdf", 25 * mb), ("clip.mp4", 25 * mb)] {
        #expect(file(name, limit).opensInline, "\(name)")
        #expect(!file(name, limit + 1).opensInline, "\(name)")
    }
    #expect(!file("archive.zip", 1).opensInline)
}

@Test func aVideoIsCheckedByItsBytesNotItsName() throws {
    let fixture = { (name: String) in
        try Data(contentsOf: #require(Bundle.module.url(forResource: name, withExtension: nil, subdirectory: "Fixtures")))
    }
    for name in ["vp8.webm", "vp9.webm"] { #expect(try SharedFile.VideoContainer(fixture(name)) == .webM, "\(name)") }
    #expect(try SharedFile.VideoContainer(fixture("h264.mp4")) == .isoMedia)
    let box = { (type: String) in Data([0, 0, 0, 0x20]) + Data(type.utf8) + Data("isom".utf8) }
    for type in ["moov", "mdat", "wide"] { #expect(SharedFile.VideoContainer(box(type)) == .isoMedia, "\(type)") }
    #expect(SharedFile.VideoContainer(Data("PK\u{3}\u{4}zip renamed .mp4".utf8)) == nil)
    #expect(SharedFile.VideoContainer(Data("plain text".utf8)) == nil)
    #expect(SharedFile.VideoContainer(Data([0, 0, 0, 0x20, 0x66])) == nil)
}

@Test func generatedFileFailuresRemovePartialBytes() async throws {
    let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    defer { try? FileManager.default.removeItem(at: directory) }
    let bytes = Data([0, 255, 42])
    let writer = FileDownload()
    for transport in [FileTransport(bytes: bytes, truncate: true), FileTransport(bytes: bytes, wrongSize: true)] {
        await #expect(throws: FileDownloadError.self) {
            try await writer.save(client: HostClient(transport: transport), entryId: "entry", file: metadata(bytes), directory: directory) { _ in }
        }
        #expect(try FileManager.default.contentsOfDirectory(atPath: directory.path).isEmpty)
    }
    let bad = SharedFile(id: "id", name: "file.bin", size: Int64(bytes.count), sha256: String(repeating: "0", count: 64))
    await #expect(throws: FileDownloadError.self) {
        try await writer.save(client: HostClient(transport: FileTransport(bytes: bytes)), entryId: "entry", file: bad, directory: directory) { _ in }
    }
    #expect(try FileManager.default.contentsOfDirectory(atPath: directory.path).isEmpty)
}

@Test func generatedFileCancellationRemovesPartialBytes() async throws {
    let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    defer { try? FileManager.default.removeItem(at: directory) }
    let bytes = Data(repeating: 42, count: FileDownload.chunkSize + 1)
    let writer = FileDownload()
    let task = Task {
        try await writer.save(client: HostClient(transport: FileTransport(bytes: bytes)), entryId: "entry", file: metadata(bytes), directory: directory) { _ in
            withUnsafeCurrentTask { $0?.cancel() }
        }
    }
    await #expect(throws: CancellationError.self) { try await task.value }
    #expect(try FileManager.default.contentsOfDirectory(atPath: directory.path).isEmpty)
}

@Test func generatedFileMetadataIsAdditiveAndRejectsUnsafeNames() throws {
    let old = try JSONDecoder().decode(EntryData.self, from: Data(#"{"text":"hello","final":true}"#.utf8))
    #expect(old.files == nil)
    for name in ["../escape", "bad\\file", "..", "\n"] {
        #expect(throws: FileDownloadError.self) { try metadata(Data(), name: name).validate() }
    }
    try metadata(Data(), name: ".hidden").validate()
}
