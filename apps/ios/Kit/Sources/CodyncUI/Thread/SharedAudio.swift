import AVFoundation
import CodyncKit
import Observation

/// Plays a sound a bot shared (a voice note), fetched once from the computer; one card's player.
@MainActor
@Observable
final class SharedAudio {
    private(set) var playing = false
    @ObservationIgnored private var player: AVAudioPlayer?
    @ObservationIgnored private var finished: Finished?

    func toggle(_ file: SharedFile, entryId: String, model: BotStore) async {
        if let player, player.isPlaying {
            player.pause()
            playing = false
            return
        }
        do {
            let player = try await prepared(file, entryId: entryId, model: model)
            // Heard with the ringer off too; a call or dictation keeps its own session.
            let session = AVAudioSession.sharedInstance()
            if session.category != .playAndRecord { try session.setCategory(.playback) }
            try session.setActive(true)
            playing = player.play()
        } catch {
            model.lastError = "Couldn't play \(file.name)."
        }
    }

    func stop() {
        player?.stop()
        playing = false
    }

    private func prepared(_ file: SharedFile, entryId: String, model: BotStore) async throws -> AVAudioPlayer {
        if let player { return player }
        guard let data = await model.sharedFileData(file, entryId: entryId) else { throw CocoaError(.fileReadUnknown) }
        let player = try AVAudioPlayer(data: data)
        let finished = Finished { [weak self] in Task { @MainActor in self?.playing = false } }
        player.delegate = finished
        (self.player, self.finished) = (player, finished)
        return player
    }
}

/// Player delegate: a note that played to its end shows Play again.
private final class Finished: NSObject, AVAudioPlayerDelegate, Sendable {
    let onFinish: @Sendable () -> Void

    init(onFinish: @escaping @Sendable () -> Void) { self.onFinish = onFinish }

    func audioPlayerDidFinishPlaying(_ player: AVAudioPlayer, successfully flag: Bool) { onFinish() }
}
