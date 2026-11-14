import assert from 'node:assert/strict'
import test from 'node:test'
import { fallbackSvg } from './icon-fallbacks.ts'

test('a filled play or pause fallback is a solid disc, not the outline it hovers over', () => {
  for (const name of ['play.circle', 'pause.circle']) {
    const filled = fallbackSvg(`${name}.fill`)
    assert.notEqual(filled, fallbackSvg(name))
    assert.match(filled, /fill="currentColor"/)
  }
})
