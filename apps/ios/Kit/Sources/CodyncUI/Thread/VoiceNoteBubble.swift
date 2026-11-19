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
    /// Whether the phone's network counts bytes (cellular, a hotspot): a bigger note's sound waits for a tap there.
    @State private var cost = NetworkCost.shared

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
        // The sound itself is fetched too, so the first tap plays at once — unless the fetch would cost (a metered
        // network and a big note), where a tap still fetches. A chat shown before the computer connects reads nothing.
        .task(id: Preload(known: file.durationMs, online: model.connection == .online, expensive: cost.expensive)) {
            audio.setKnownDuration(file.durationMs.map { TimeInterval($0) / 1000 })
            if SharedAudio.preloads(online: model.connection == .online, expensive: cost.expensive, bytes: file.size) {
                await audio.preload(file, entryId: entry.id, model: model)
            }
        }
    }

    /// The play/pause button, the progress bar and its clock on one row; a failed play puts its error in the bar's
    /// place, a failed download only colours the clock's caption. Download is in the message's menu.
    private var player: some View {
        let state = model.fileDownloads.states[file.id]
        let caption = FileDownloadCard.voiceNoteDetail(state, play: audio.state, elapsed: audio.elapsed, duration: audio.duration)
        let failed = audio.state == .failed
        let downloadFailed = if case .failed = state { true } else { false }
        // The 44 pt hit area overflows the 24 pt glyph, so the glyph lines up with the text under it.
        return HStack(spacing: 12) {
            AudioPlayButton(state: audio.state, action: play).padding(-10)
            if failed {
                Text(caption).font(.caption2).foregroundStyle(Palette.danger)
                    .frame(maxWidth: .infinity, alignment: .center)
                    .padding(.trailing, 12)
            } else {
                AudioProgress(elapsed: audio.elapsed, duration: audio.duration ?? 0, seek: audio.seek)
                // The clock keeps exactly its own room — the caption's shape for this length — whether the length is
                // known yet or not: the gap to the bar's end stays the row's spacing from the first frame, when the
                // length arrives, and as the clock runs.
                ZStack(alignment: .trailing) {
                    Text(verbatim: FileDownloadCard.voiceNoteRoom(audio.duration)).hidden()
                    Text(verbatim: caption).foregroundStyle(downloadFailed ? Palette.danger : Palette.secondary)
                }
                .font(.caption2.monospacedDigit())
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        // One element reads the whole player; its buttons become named actions.
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(caption.isEmpty ? name : "\(name), \(caption)")
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

/// What the bubble's preload follows: the computer's stamp of the length, and whether the network lets the sound
/// itself be fetched yet.
private struct Preload: Equatable {
    let known: Int64?
    let online: Bool
    let expensive: Bool?
}
