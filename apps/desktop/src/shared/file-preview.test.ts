import assert from 'node:assert/strict'
import { test } from 'node:test'
import { byteLimit, filePreview, isVideoData, opensInline } from './file-preview.ts'
import type { SharedFile } from './models.ts'

const MB = 1024 * 1024
const shared = (name: string, size = 1): SharedFile => ({ id: 'f', name, size, sha256: '0'.repeat(64) })

test('a shared Markdown, text, PDF or video file names its preview kind and byte limit', () => {
  assert.deepEqual(filePreview(shared('notes.md')), { kind: 'markdown', maxBytes: 10 * MB })
  assert.deepEqual(filePreview(shared('report.markdown')), { kind: 'markdown', maxBytes: 10 * MB })
  for (const name of ['server.log', 'data.csv', 'x.json']) assert.deepEqual(filePreview(shared(name)), { kind: 'text', maxBytes: 10 * MB }, name)
  assert.deepEqual(filePreview(shared('spec.pdf')), { kind: 'pdf', maxBytes: 25 * MB })
  for (const name of ['clip.mp4', 'clip.MOV', 'clip.m4v', 'clip.webm']) assert.deepEqual(filePreview(shared(name)), { kind: 'video', maxBytes: 25 * MB }, name)
})

test('the preview kind ignores the extension\'s case and misses other files', () => {
  assert.equal(filePreview(shared('NOTES.MD'))?.kind, 'markdown')
  for (const name of ['Shot.PNG', 'voice.m4a', 'memo', 'md', 'report.constructor', 'x.__proto__', 'a.toString']) {
    assert.equal(filePreview(shared(name)), null, name)
  }
})

test('a file at its limit opens inside the app; one byte over does not', () => {
  for (const [name, limit] of [['a.md', 10 * MB], ['a.csv', 10 * MB], ['a.json', 10 * MB], ['a.pdf', 25 * MB], ['a.mp4', 25 * MB]] as const) {
    assert.equal(opensInline(shared(name, limit)), true, name)
    assert.equal(opensInline(shared(name, limit + 1)), false, name)
  }
  assert.equal(opensInline(shared('build.zip')), false)
})

test('a video is checked by its bytes, not its name', () => {
  const box = (type: string) => new Uint8Array([0, 0, 0, 0x20, ...new TextEncoder().encode(type + 'isom')])
  for (const type of ['ftyp', 'moov', 'mdat', 'wide']) assert.equal(isVideoData(box(type)), true, type)
  assert.equal(isVideoData(new Uint8Array([0x1a, 0x45, 0xdf, 0xa3, 0x9f, 0x42, 0x86, 0x81])), true)
  assert.equal(isVideoData(new TextEncoder().encode('PK\x03\x04zip renamed .mp4')), false)
  assert.equal(isVideoData(new Uint8Array([0, 0, 0, 0x20, 0x66])), false)
})

test('the too-big note names the limit in MB', () => {
  assert.equal(byteLimit(10 * MB), '10 MB')
  assert.equal(byteLimit(25 * MB), '25 MB')
})
