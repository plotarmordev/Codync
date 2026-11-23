// Runs Maestro with the given arguments, then trims its run folders in ~/.maestro/tests: each keeps only its
// screenshots (failure and `takeScreenshot` PNGs) and maestro.log, and runs older than a day are removed. Maestro
// itself keeps every run, with tens of MB of device logs each, for 14 days (docs/guides/test-stack.md#phones-maestro).
//   node tools/maestro.mjs --device "<simulator udid>,<adb serial>" test … apps/maestro
import { spawnSync } from 'node:child_process'
import { readdirSync, rmSync, statSync } from 'node:fs'
import { homedir } from 'node:os'
import { join } from 'node:path'

const DAY_MS = 24 * 60 * 60 * 1000
const kept = (name) => name.endsWith('.png') || name === 'maestro.log'

// Deletes every file `kept` rejects under `dir`, then any folder left empty.
const trim = (dir) => {
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const path = join(dir, entry.name)
    if (entry.isDirectory()) {
      trim(path)
      if (readdirSync(path).length === 0) rmSync(path, { recursive: true })
    } else if (!kept(entry.name)) {
      rmSync(path)
    }
  }
}

export const cleanRuns = (root, now = Date.now()) => {
  for (const entry of readdirSync(root, { withFileTypes: true })) {
    if (!entry.isDirectory()) continue
    const run = join(root, entry.name)
    if (now - statSync(run).mtimeMs > DAY_MS) rmSync(run, { recursive: true })
    else trim(run)
  }
}

if (import.meta.main) {
  const result = spawnSync('maestro', process.argv.slice(2), { stdio: 'inherit' })
  if (result.error) throw result.error
  cleanRuns(join(homedir(), '.maestro', 'tests'))
  process.exit(result.status ?? 1)
}
