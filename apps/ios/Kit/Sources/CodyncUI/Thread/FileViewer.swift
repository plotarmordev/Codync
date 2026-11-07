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

/// A Markdown, text or PDF file a bot shared, read in place. A file over its kind's `maxBytes` gets a
/// notice instead of a fetch; Download always works, shown by the viewer's own share sheet.
struct FileViewer: View {
    let file: SharedFile
    let entryId: String
    let botId: String
    @Environment(BotStore.self) private var model
    @State private var content = Content.loading

    private enum Content {
        case loading, failed, tooBig, notUTF8, markdown(String), text(String), pdf(Data)
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
        .fileExportSheet(model, botId: botId)
    }

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
        case .markdown(let source):
            ScrollView { MarkdownText(source).padding(20).frame(maxWidth: .infinity, alignment: .leading) }
        case .text(let source):
            ScrollView {
                Text(source).font(.system(.footnote, design: .monospaced)).textSelection(.enabled)
                    .padding(20).frame(maxWidth: .infinity, alignment: .leading)
            }
        case .pdf(let data):
            PDFViewer(data: data)
        }
    }

    private func notice(_ message: String) -> some View {
        Text(message)
            .font(.subheadline).foregroundStyle(Palette.secondary)
            .multilineTextAlignment(.center).textSelection(.enabled)
            .padding(20).frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    /// "512 KB" / "20 MB" in the too-big notice.
    private func tooBig(_ kind: SharedFile.FilePreview) -> String {
        let limit = kind.maxBytes >= 1024 * 1024 ? "\(kind.maxBytes / (1024 * 1024)) MB" : "\(kind.maxBytes / 1024) KB"
        return L10n.tr("This file is too big to open here (over {limit}). Download it to open it in another app.")
            .replacingOccurrences(of: "{limit}", with: limit)
    }

    private func load(_ kind: SharedFile.FilePreview) async {
        guard case .loading = content else { return }
        guard file.opensInline else { content = .tooBig; return }
        guard let data = await model.sharedFileData(file, entryId: entryId) else { content = .failed; return }
        switch kind {
        case .pdf:
            content = .pdf(data)
        case .markdown, .text:
            guard let source = String(data: data, encoding: .utf8) else { content = .notUTF8; return }
            content = kind == .markdown ? .markdown(source) : .text(source)
        }
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
