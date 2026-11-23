// Checks how tools/maestro.mjs trims Maestro's run folders: node --test tools/maestro-test.mjs
import assert from 'node:assert/strict'
import { mkdirSync, mkdtempSync, readdirSync, utimesSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { dirname, join, relative } from 'node:path'
import { test } from 'node:test'
import { cleanRuns } from './maestro.mjs'

const files = (dir) =>
  readdirSync(dir, { recursive: true, withFileTypes: true })
    .filter((entry) => entry.isFile())
    .map((entry) => relative(dir, join(entry.parentPath, entry.name)))
    .sort()

const makeRun = (root, name, paths) => {
  for (const path of paths) {
    mkdirSync(dirname(join(root, name, path)), { recursive: true })
    writeFileSync(join(root, name, path), '')
  }
  return join(root, name)
}

test('a run keeps only its screenshots and maestro logs', () => {
  const root = mkdtempSync(join(tmpdir(), 'maestro-'))
  makeRun(root, 'run', [
    'maestro.log',
    'xctest_runner_1.log',
    'welcome/manifest.json',
    'welcome/commands.json',
    'welcome/logs/maestro.log',
    'welcome/logs/device-simulator.log',
    'welcome/screen-hierarchy/step-4.json',
    'welcome/screenshots/step-4.png',
    'welcome/takeScreenshot/welcome-shown.png',
  ])
  cleanRuns(root)
  assert.deepEqual(files(root), [
    'run/maestro.log',
    'run/welcome/logs/maestro.log',
    'run/welcome/screenshots/step-4.png',
    'run/welcome/takeScreenshot/welcome-shown.png',
  ])
  assert.deepEqual(readdirSync(join(root, 'run', 'welcome')).sort(), ['logs', 'screenshots', 'takeScreenshot'])
})

test('runs older than a day are removed, newer ones stay', () => {
  const root = mkdtempSync(join(tmpdir(), 'maestro-'))
  const now = Date.now()
  const old = makeRun(root, 'old', ['maestro.log'])
  makeRun(root, 'recent', ['maestro.log'])
  const dayAndMinuteAgo = new Date(now - 24 * 60 * 60 * 1000 - 60 * 1000)
  utimesSync(old, dayAndMinuteAgo, dayAndMinuteAgo)
  cleanRuns(root, now)
  assert.deepEqual(readdirSync(root), ['recent'])
})
