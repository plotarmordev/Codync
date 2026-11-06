import CodyncKit
import SwiftUI
import UIKit

struct FileDownloadCard: View {
    let entry: Entry
    let file: SharedFile
    @Environment(BotStore.self) private var model
    @State private var viewing = false
    @State private var audio = SharedAudio()

    var body: some View {
        let download = model.fileDownloads.states[file.id]
        let caption = Self.detail(download, play: audio.state, elapsed: audio.elapsed, duration: audio.duration, size: file.size)
        VStack(alignment: .leading, spacing: 12) {
            if file.showsAsImage { preview }
            row(download, caption: caption)
        }
        .padding(14)
        .background(Palette.bubbleAgent, in: RoundedRectangle(cornerRadius: 16))
        .codyncSheet(isPresented: $viewing) { AttachmentViewer(shared: file, entryId: entry.id) }
        .onDisappear { audio.stop() }
        // One element reads the whole card; its buttons become named actions.
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("\(file.name), \(caption)")
        .accessibilityActions {
            if file.showsAsAudio {
                Button(audio.state == .playing ? L10n.tr("Pause") : L10n.tr("Play"), action: play)
            }
            if file.showsAsImage {
                Button(L10n.tr("View")) { viewing = true }
            }
            Button(busy(download) ? L10n.tr("Cancel download") : L10n.tr("Download")) { self.download(download) }
        }
    }

    /// The picture itself; tap for full size.
    private var preview: some View {
        AttachmentImage(shared: file, entryId: entry.id, maxPixels: 720)
            .clipShape(RoundedRectangle(cornerRadius: 10, style: .continuous))
            .contentShape(Rectangle())
            .onTapGesture { viewing = true }
            .frame(maxWidth: .infinity, maxHeight: 280, alignment: .leading)
    }

    private func row(_ download: FileDownloads.State?, caption: String) -> some View {
        HStack(spacing: 12) {
            if file.showsAsAudio { playButton } else {
                Image(systemName: AttachmentIcon.symbol(file.name)).font(.system(size: 24)).foregroundStyle(Palette.secondary)
                    .frame(width: 24)
            }
            VStack(alignment: .leading, spacing: 3) {
                Text(file.name).font(.subheadline.weight(.semibold)).foregroundStyle(Palette.text).lineLimit(2)
                Text(caption).font(.caption2).foregroundStyle(audio.state == .failed ? Palette.danger : Palette.secondary)
                if audio.duration != nil { progress }
            }
            Spacer(minLength: 0)
            Button { self.download(download) } label: {
                Image(systemName: busy(download) ? "xmark" : "arrow.down.to.line")
                    .font(.system(size: 20)).frame(width: 44, height: 44).contentShape(Rectangle())
            }
            .buttonStyle(.plain)
        }
    }

    /// A voice note's play/pause (a spinner while it loads), in place of its file icon. The 44 pt hit area overflows
    /// the 24 pt glyph's slot, so the name lines up with the other file cards.
    private var playButton: some View {
        Button(action: play) {
            Group {
                if audio.state == .loading {
                    Spinner(size: 24)
                } else {
                    Image(systemName: audio.state == .playing ? "pause.circle" : "play.circle")
                        .font(.system(size: 24)).foregroundStyle(Palette.text)
                        .contentTransition(.symbolEffect(.replace))
                }
            }
            .frame(width: 44, height: 44)
            .contentShape(Rectangle())
            .padding(-10)
        }
        .buttonStyle(.plain)
    }

    /// The position in the sound, once its length is known.
    private var progress: some View {
        Capsule()
            .fill(Palette.tertiary)
            .frame(height: 2.5)
            .overlay(alignment: .leading) {
                GeometryReader { proxy in
                    Capsule().fill(Palette.text)
                        .frame(width: proxy.size.width * min(1, audio.elapsed / max(1, audio.duration ?? 1)))
                }
            }
            .padding(.top, 3)
    }

    private func play() {
        Task { await audio.toggle(file, entryId: entry.id, model: model) }
    }

    private func download(_ state: FileDownloads.State?) {
        Motion.animate {
            if case .downloading = state { model.fileDownloads.cancel(file.id) }
            else { model.fileDownloads.save(client: model.client, entry: entry, file: file) }
        }
    }

    private func busy(_ state: FileDownloads.State?) -> Bool {
        if case .downloading = state { return true }
        return false
    }

    /// What the card says under the name: a failed play, then the download, then the sound's position, then its size.
    static func detail(_ download: FileDownloads.State?, play: SharedAudio.State = .idle,
                       elapsed: TimeInterval = 0, duration: TimeInterval? = nil, size bytes: Int64) -> String {
        if play == .failed { return L10n.tr("Couldn't play. Tap to retry.") }
        let size = ByteCountFormatter.string(fromByteCount: bytes, countStyle: .file)
        switch download {
        case .downloading(let received):
            let received = ByteCountFormatter.string(fromByteCount: received, countStyle: .file)
            return L10n.tr("{received} of {size}")
                .replacingOccurrences(of: "{received}", with: received)
                .replacingOccurrences(of: "{size}", with: size)
        case .failed(let message): return "\(L10n.tr(message)) \(L10n.tr("Tap to retry."))"
        case .saved, nil: break
        }
        if let duration { return "\(SharedAudio.timeText(elapsed)) / \(SharedAudio.timeText(duration))" }
        if case .saved = download { return "\(size) · \(L10n.tr("Downloaded"))" }
        return size
    }
}

extension View {
    /// The share sheet for a file of this chat once it's downloaded (`FileDownloads.export`). Off (`active`) while
    /// a sheet over this view presents it instead.
    func fileExportSheet(_ model: BotStore, botId: String, active: Bool = true) -> some View {
        codyncSheet(item: Binding<FileDownloads.Export?>(get: {
            guard active, let item = model.fileDownloads.export, item.botId == botId else { return nil }
            return item
        }, set: { if $0 == nil { model.fileDownloads.dismissExport() } })) { item in
            FileExportSheet(url: item.url) { model.fileDownloads.dismissExport() }
        }
    }
}

/// iOS owns save/export destinations through its native share sheet.
struct FileExportSheet: UIViewControllerRepresentable {
    let url: URL
    let completed: () -> Void
    func makeUIViewController(context: Context) -> UIActivityViewController {
        let controller = UIActivityViewController(activityItems: [url], applicationActivities: nil)
        controller.completionWithItemsHandler = { _, _, _, _ in completed() }
        return controller
    }
    func updateUIViewController(_ controller: UIActivityViewController, context: Context) {}
}
