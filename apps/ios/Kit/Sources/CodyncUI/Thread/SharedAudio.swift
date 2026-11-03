import AVFoundation
import CodyncKit
import Observation

/// Plays a sound a bot shared (a voice note), fetched once from the computer; one card's player.
@MainActor
@Observable
final class SharedAudio {
    private(set) var playing = false
    @ObservationIgnored private var player: AVAudioPlayer?
    @ObservationIgnored private var done: PlayDone?
    /// A tap while the sound loads is dropped, so it never starts a second player.
    @ObservationIgnored private var loading = false
    /// Whether this player turned the audio session on (a call or dictation keeps its own).
    @ObservationIgnored private var ownsSession = false
    /// Bumped by `stop()`: a load still running when the card went doesn't start playing.
    @ObservationIgnored private var generation = 0

    func toggle(_ file: SharedFile, entryId: String, model: BotStore) async {
        if let player, player.isPlaying {
            player.pause()
            finished()
            return
        }
        guard !loading else { return }
        loading = true
        defer { loading = false }
        let started = generation
        do {
            let player = try await prepared(file, entryId: entryId, model: model)
            guard generation == started else { return }
            // Heard with the ringer off too.
            let session = AVAudioSession.sharedInstance()
            if session.category != .playAndRecord {
                try session.setCategory(.playback)
                try session.setActive(true)
                ownsSession = true
            }
            playing = player.play()
        } catch {
            model.lastError = "Couldn't play \(file.name)."
        }
    }

    func stop() {
        generation += 1
        player?.stop()
        finished()
    }

    /// Paused, stopped or played to its end: other apps' audio may pick up again.
    private func finished() {
        playing = false
        guard ownsSession else { return }
        ownsSession = false
        try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)
    }

    private func prepared(_ file: SharedFile, entryId: String, model: BotStore) async throws -> AVAudioPlayer {
        if let player { return player }
        guard let data = await model.sharedFileData(file, entryId: entryId) else { throw CocoaError(.fileReadUnknown) }
        let player = try AVAudioPlayer(data: data)
        let done = PlayDone { [weak self] in Task { @MainActor in self?.finished() } }
        player.delegate = done
        (self.player, self.done) = (player, done)
        return player
    }
}
