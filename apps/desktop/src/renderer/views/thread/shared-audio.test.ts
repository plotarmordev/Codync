import assert from 'node:assert/strict'
import { test } from 'node:test'
import { SharedAudio, type SharedAudioPlatform, type SoundEvents } from './shared-audio.ts'

/** A card's player whose fetch from the computer the test answers when it chooses. */
function setup() {
  const log: string[] = []
  const loads: ((data: Uint8Array | null) => void)[] = []
  let events!: SoundEvents
  const sound = {
    play: async () => { log.push('play'); events.playing(true) },
    pause: () => { log.push('pause'); events.playing(false) },
    dispose: () => void log.push('dispose'),
  }
  const platform: SharedAudioPlatform = {
    load: () => new Promise<Uint8Array | null>((answer) => loads.push(answer)),
    open: (_, wired) => {
      log.push('open')
      events = wired
      return sound
    },
  }
  return { audio: new SharedAudio(platform, () => {}), log, loads, events: () => events, sound }
}

test('a second tap while the sound loads plays it once', async () => {
  const { audio, log, loads } = setup()
  const first = audio.toggle()
  await audio.toggle()
  assert.equal(audio.snapshot.phase, 'loading')
  assert.equal(loads.length, 1)
  loads[0](new Uint8Array([1]))
  await first
  assert.deepEqual(log, ['open', 'play'])
  assert.equal(audio.snapshot.phase, 'playing')
  await audio.toggle()
  assert.equal(audio.snapshot.phase, 'paused')
})

test('a card that goes while its sound loads plays nothing', async () => {
  const { audio, log, loads } = setup()
  const tap = audio.toggle()
  audio.dispose()
  loads[0](new Uint8Array([1]))
  await tap
  assert.deepEqual(log, [])
  assert.equal(audio.snapshot.phase, 'idle')
})

test('a sound that fails to load fails the card, and the next tap loads it again', async () => {
  const { audio, log, loads } = setup()
  const tap = audio.toggle()
  loads[0](null)
  await tap
  assert.equal(audio.snapshot.phase, 'failed')
  const retry = audio.toggle()
  assert.equal(loads.length, 2, 'the retry fetches again')
  loads[1](new Uint8Array([1]))
  await retry
  assert.deepEqual(log, ['open', 'play'])
  assert.equal(audio.snapshot.phase, 'playing')
})

test('a sound whose play() starts then rejects ends failed, not playing', async () => {
  const { audio, log, loads, events, sound } = setup()
  sound.play = async () => {
    log.push('play')
    events().playing(true) // <audio>.play() fires `play` before its promise rejects
    throw new Error('decode failed')
  }
  const tap = audio.toggle()
  loads[0](new Uint8Array([1]))
  await tap
  assert.deepEqual(log, ['open', 'play', 'pause', 'dispose'])
  assert.equal(audio.snapshot.phase, 'failed')
  const retry = audio.toggle()
  assert.equal(loads.length, 2, 'the retry fetches again')
  loads[1](null)
  await retry
})

test('a loaded sound shows its clock and resets at the end', async () => {
  const { audio, loads, events } = setup()
  const tap = audio.toggle()
  loads[0](new Uint8Array([1]))
  await tap
  events().time(7, 48)
  assert.deepEqual(audio.snapshot, { phase: 'playing', elapsed: 7, duration: 48 })
  events().ended()
  assert.deepEqual(audio.snapshot, { phase: 'idle', elapsed: 0, duration: 48 })
})
