import CodyncKit
import SwiftUI

/// A sound the chat plays, the same bubble for both kinds: a voice note inside a message (the player at the top,
/// the message under it) and a sound shared on its own (the file's name in the message's place). The way a
/// messaging app shows a voice note, with or without a caption.
struct VoiceNoteBubble<Below: View>: View {
    let entry: Entry
    let file: SharedFile
    /// What VoiceOver calls the player: "Voice note", or the sound's own name when it stands alone.
    let name: String
    let below: Below
    @Environment(BotStore.self) private var model
    @State private var audio: SharedAudio

    init(entry: Entry, file: SharedFile, name: String, @ViewBuilder below: () -> Below) {
        self.entry = entry
        self.file = file
        self.name = name
        self.below = below()
        // The computer's stamp of the sound's length, so the bar and its clock show from the first frame.
        _audio = State(initialValue: SharedAudio(known: file.durationMs.map { TimeInterval($0) / 1000 }))
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            player
            below
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 10)
        .background(Palette.bubbleAgent, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
        .onDisappear { audio.stop() }
        // A backfilled note's length arrives after the bubble mounted: show it without waiting for the sound to load.
        .onChange(of: file.durationMs) { _, millis in audio.setKnownDuration(millis.map { TimeInterval($0) / 1000 }) }
    }

    /// The play/pause button, the progress bar and its clock on one row; download is in the message's menu.
    private var player: some View {
        let state = model.fileDownloads.states[file.id]
        let caption = FileDownloadCard.detail(state, play: audio.state, elapsed: audio.elapsed, duration: audio.duration, size: file.size)
        // The 44 pt hit area overflows the 24 pt glyph, so the glyph lines up with the text under it.
        return HStack(spacing: 12) {
            AudioPlayButton(state: audio.state, action: play).padding(-10)
            AudioProgress(elapsed: audio.elapsed, duration: audio.duration ?? 0, seek: audio.seek)
            // Steady digits, so the bar's end doesn't move as the clock runs.
            Text(caption).font(.caption2.monospacedDigit()).foregroundStyle(audio.state == .failed ? Palette.danger : Palette.secondary)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        // One element reads the whole player; its buttons become named actions.
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("\(name), \(caption)")
        .accessibilityAction { play() }
        .accessibilityActions {
            Button(audio.state == .playing ? L10n.tr("Pause") : L10n.tr("Play"), action: play)
            Button(model.fileDownloads.isDownloading(file.id) ? L10n.tr("Cancel download") : L10n.tr("Download"), action: self.download)
        }
        .audioSeekAdjustable(audio)
    }

    private func play() {
        Task { await audio.toggle(file, entryId: entry.id, model: model) }
    }

    private func download() {
        model.fileDownloads.toggle(client: model.client, entry: entry, file: file)
    }
}
