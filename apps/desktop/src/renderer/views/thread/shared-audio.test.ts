import assert from 'node:assert/strict'
import { test } from 'node:test'
import { SharedAudio, type SharedAudioPlatform } from './shared-audio.ts'

/** A card's player whose fetch from the computer the test answers when it chooses. */
function setup() {
  const log: string[] = []
  const loads: ((data: Uint8Array | null) => void)[] = []
  const platform: SharedAudioPlatform = {
    load: () => new Promise((answer) => loads.push(answer)),
    open: (_, changed) => {
      log.push('open')
      return {
        play: async () => { log.push('play'); changed(true) },
        pause: () => { log.push('pause'); changed(false) },
        dispose: () => void log.push('dispose'),
      }
    },
    failed: () => void log.push('failed'),
  }
  return { audio: new SharedAudio(platform, () => {}), log, loads }
}

test('a second tap while the sound loads plays it once', async () => {
  const { audio, log, loads } = setup()
  const first = audio.toggle()
  await audio.toggle()
  assert.equal(loads.length, 1)
  loads[0](new Uint8Array([1]))
  await first
  assert.deepEqual(log, ['open', 'play'])
  assert.ok(audio.playing)
  await audio.toggle()
  assert.ok(!audio.playing)
})

test('a card that goes while its sound loads plays nothing', async () => {
  const { audio, log, loads } = setup()
  const tap = audio.toggle()
  audio.dispose()
  loads[0](new Uint8Array([1]))
  await tap
  assert.deepEqual(log, [])
  assert.ok(!audio.playing)
})

test('an unreadable sound says so', async () => {
  const { audio, log, loads } = setup()
  const tap = audio.toggle()
  loads[0](null)
  await tap
  assert.deepEqual(log, ['failed'])
})
