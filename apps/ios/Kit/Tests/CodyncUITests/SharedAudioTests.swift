import Foundation
import Testing
@testable import CodyncKit
@testable import CodyncUI

/// A sound's fetch from the computer that the test answers when it chooses.
@MainActor private final class HeldLoad {
    private var waiting: CheckedContinuation<Data?, Never>?
    private(set) var calls = 0

    func call() async -> Data? {
        calls += 1
        return await withCheckedContinuation { waiting = $0 }
    }

    func asked() async -> Bool { await until { self.waiting != nil } }

    func answer(_ data: Data?) {
        waiting?.resume(returning: data)
        waiting = nil
    }
}

/// A second of silence as a 16-bit mono WAV: a sound `AVAudioPlayer` would play.
private func silentWAV() -> Data {
    let rate: UInt32 = 8000
    let samples = Data(count: Int(rate) * 2)
    func le<T: FixedWidthInteger>(_ value: T) -> Data { withUnsafeBytes(of: value.littleEndian) { Data($0) } }
    var wav = Data("RIFF".utf8) + le(UInt32(36 + samples.count)) + Data("WAVEfmt ".utf8)
    wav += le(UInt32(16)) + le(UInt16(1)) + le(UInt16(1)) + le(rate) + le(rate * 2) + le(UInt16(2)) + le(UInt16(16))
    return wav + Data("data".utf8) + le(UInt32(samples.count)) + samples
}

/// A voice note's player whose fetch `load` holds, on a store to report errors to.
@MainActor private func playing(_ body: (SharedAudio, HeldLoad, BotStore, SharedFile) async -> Void) async {
    let suite = "SharedAudioTests.\(UUID().uuidString)"
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let storage = SharedStore.Context(accountID: UUID().uuidString, suite: suite)
    let store = BotStore(computer: randomComputer("Mac"), storage: storage) { _ in FakeRemote(.connecting) }
    defer { store.retire() }
    let load = HeldLoad()
    let audio = SharedAudio { _, _, _ in await load.call() }
    await body(audio, load, store, SharedFile(id: UUID().uuidString, name: "note.wav", size: 0, sha256: ""))
}

@MainActor @Test func aSecondTapWhileTheSoundLoadsPlaysItOnce() async {
    await playing { audio, load, store, note in
        let first = Task { await audio.toggle(note, entryId: "entry", model: store) }
        #expect(await load.asked())
        await audio.toggle(note, entryId: "entry", model: store)
        load.answer(silentWAV())
        await first.value

        #expect(load.calls == 1)
        #expect(audio.playing)
        await audio.toggle(note, entryId: "entry", model: store)
        #expect(!audio.playing)
    }
}

@MainActor @Test func aVoiceNoteWhoseCardGoesWhileItLoadsNeverPlays() async {
    await playing { audio, load, store, note in
        let tap = Task { await audio.toggle(note, entryId: "entry", model: store) }
        #expect(await load.asked())
        audio.stop()
        load.answer(silentWAV())
        await tap.value

        #expect(!audio.playing)
        #expect(store.lastError == nil)
    }
}
