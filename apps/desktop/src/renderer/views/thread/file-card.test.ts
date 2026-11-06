import assert from 'node:assert/strict'
import { test } from 'node:test'
import type { SharedFile } from '@shared/models'
import type { DownloadState } from '../../store/file-downloads.ts'
import { byteCount, cardDetail, clock } from './file-card.ts'
import type { AudioSnapshot } from './shared-audio.ts'

const file = (size: number): SharedFile => ({ id: 'f1', name: 'venue-floor-plan.pdf', size, sha256: 'a'.repeat(64) })
const audio = (phase: AudioSnapshot['phase'], elapsed = 0, duration = 0): AudioSnapshot => ({ phase, elapsed, duration })

test('byteCount matches ByteCountFormatter(.file): decimal units, no needless zeros', () => {
  const expected: [number, string][] = [
    [0, 'Zero KB'], [1, '1 byte'], [43, '43 bytes'], [999, '999 bytes'],
    [1000, '1 KB'], [1499, '1 KB'], [1500, '2 KB'], [47600, '48 KB'], [64400, '64 KB'], [380030, '380 KB'],
    [999500, '1 MB'], [1049999, '1 MB'], [1050000, '1.1 MB'], [1234567, '1.2 MB'], [12345678, '12.3 MB'],
    [1000000000, '1 GB'], [1234567890, '1.23 GB'],
  ]
  for (const [bytes, text] of expected) assert.equal(byteCount(bytes), text, `${bytes}`)
})

test('clock writes m:ss, and h:mm:ss past an hour', () => {
  assert.equal(clock(0), '0:00')
  assert.equal(clock(7.9), '0:07')
  assert.equal(clock(48), '0:48')
  assert.equal(clock(600), '10:00')
  assert.equal(clock(3725), '1:02:05')
})

test('a card that could not play says so before anything else', () => {
  const downloading: DownloadState = { kind: 'downloading', received: 500 }
  assert.equal(cardDetail(downloading, file(1000), audio('failed')), `Couldn't play. Click to retry.`)
})

test('a downloading or failed download beats the sound clock', () => {
  const downloading: DownloadState = { kind: 'downloading', received: 500 }
  const failed: DownloadState = { kind: 'failed', message: 'No space left.' }
  assert.equal(cardDetail(downloading, file(1000), audio('playing', 7, 48)), '500 bytes of 1 KB')
  assert.equal(cardDetail(failed, file(1000), audio('playing', 7, 48)), 'No space left.')
})

test('a loaded sound shows elapsed of duration while playing, paused or done', () => {
  for (const phase of ['playing', 'paused', 'idle'] as const) {
    assert.equal(cardDetail(undefined, file(64400), audio(phase, 7, 48)), '0:07 / 0:48')
  }
})

test('a card without a loaded sound or download keeps the file detail', () => {
  assert.equal(cardDetail(undefined, file(380030)), '380 KB')
  assert.equal(cardDetail(undefined, file(380030), audio('idle')), '380 KB')
  assert.equal(cardDetail(undefined, file(380030), audio('loading')), '380 KB')
  const saved: DownloadState = { kind: 'saved' }
  assert.equal(cardDetail(saved, file(380030), audio('idle')), '380 KB · Saved')
})
