import AVKit
import CodyncKit
import PDFKit
import SwiftUI

/// A shared file opened in its chat's file viewer.
struct FileViewerTarget: Identifiable {
    let file: SharedFile
    let entryId: String
    let botId: String
    var id: String { file.id }
}

/// A Markdown, text, PDF or video file a bot shared, read in place. A file over its kind's `maxBytes` gets a
/// notice instead of a fetch; Download always works, shown by the viewer's own share sheet.
struct FileViewer: View {
    let file: SharedFile
    let entryId: String
    let botId: String
    @Environment(BotStore.self) private var model
    @State private var content = Content.loading

    private enum Content {
        case loading, failed, tooBig, notUTF8, notVideo
        case markdown([MarkdownBlocks.Block]), text([Substring]), pdf(Data), video(AVPlayer)
    }

    var body: some View {
        let kind = file.preview ?? .text
        VStack(spacing: 0) {
            ModalHeader(file.name) {
                IconButton("Download \(file.name)", systemImage: "arrow.down.to.line") {
                    Motion.animate {
                        model.fileDownloads.save(client: model.client, entryId: entryId, botId: botId, file: file)
                    }
                }
            }
            body(kind)
        }
        .background(Palette.background)
        .task(id: file.id) { await load(kind) }
        .onDisappear { if case .video(let player) = content { player.pause() } }
        .fileExportSheet(model, botId: botId)
    }

    /// Long files stay smooth: Markdown blocks and text lines are laid out only as they scroll into view.
    @ViewBuilder private func body(_ kind: SharedFile.FilePreview) -> some View {
        switch content {
        case .loading:
            Spinner(size: 20).frame(maxWidth: .infinity, maxHeight: .infinity)
        case .failed:
            notice(L10n.tr("Couldn't open the file."))
        case .tooBig:
            notice(tooBig(kind))
        case .notUTF8:
            notice(L10n.tr("This file isn't UTF-8 text. Download it to open it in another app."))
        case .notVideo:
            notice(L10n.tr("This video can't play here. Download it to open it in another app."))
        case .markdown(let blocks):
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 8) {
                    ForEach(Array(blocks.enumerated()), id: \.offset) { MarkdownBlock(block: $0.element) }
                }
                .textSelection(.enabled).padding(20)
            }
        case .text(let lines):
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 0) {
                    ForEach(Array(lines.enumerated()), id: \.offset) { Text($0.element) }
                }
                .font(.system(.footnote, design: .monospaced)).foregroundStyle(Palette.text)
                .textSelection(.enabled).padding(20)
            }
        case .pdf(let data):
            PDFViewer(data: data)
        case .video(let player):
            VideoPlayer(player: player).background(.black)
        }
    }

    private func notice(_ message: String) -> some View {
        Text(message)
            .font(.subheadline).foregroundStyle(Palette.secondary)
            .multilineTextAlignment(.center).textSelection(.enabled)
            .padding(20).frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    /// "10 MB" / "25 MB" in the too-big notice.
    private func tooBig(_ kind: SharedFile.FilePreview) -> String {
        L10n.tr("This file is too big to open here (over {limit}). Download it to open it in another app.")
            .replacingOccurrences(of: "{limit}", with: "\(kind.maxBytes / (1024 * 1024)) MB")
    }

    private func load(_ kind: SharedFile.FilePreview) async {
        guard case .loading = content else { return }
        guard file.opensInline else { content = .tooBig; return }
        guard let data = await model.sharedFileData(file, entryId: entryId) else { content = .failed; return }
        switch kind {
        case .pdf:
            content = .pdf(data)
        case .video:
            content = await video(data).map(Content.video) ?? .notVideo
        case .markdown, .text:
            guard let source = String(data: data, encoding: .utf8) else { content = .notUTF8; return }
            content = await Task.detached {
                kind == .markdown ? .markdown(MarkdownBlocks.parse(source)) : .text(source.split(separator: "\n", omittingEmptySubsequences: false))
            }.value
        }
    }

    /// A player for `data` when its bytes are a video AVFoundation plays. AVFoundation picks the format by the file's
    /// extension, so the checked bytes go to a temporary file with the file's own extension (one per file id).
    private func video(_ data: Data) async -> AVPlayer? {
        guard SharedFile.isVideo(data) else { return nil }
        let url = URL.temporaryDirectory.appending(path: "codync-video-\(file.id)").appendingPathExtension((file.name as NSString).pathExtension)
        guard (try? data.write(to: url, options: .atomic)) != nil else { return nil }
        let asset = AVURLAsset(url: url)
        guard (try? await asset.load(.isPlayable)) == true else { return nil }
        let player = AVPlayer(playerItem: AVPlayerItem(asset: asset))
        player.play()
        return player
    }
}

/// PDFKit's view, scaled to the sheet's width.
private struct PDFViewer: UIViewRepresentable {
    let data: Data

    func makeUIView(context: Context) -> PDFView {
        let view = PDFView()
        view.autoScales = true
        view.document = PDFDocument(data: data)
        return view
    }

    func updateUIView(_ view: PDFView, context: Context) {}
}
