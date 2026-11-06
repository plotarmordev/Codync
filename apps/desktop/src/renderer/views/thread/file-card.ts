import type { SharedFile } from '@shared/models'
import type { DownloadState } from '../../store/file-downloads'
import type { AudioSnapshot } from './shared-audio'

/** A byte count the way Apple's `ByteCountFormatter(.file)` writes it: decimal units, no needless zeros. */
/** Each unit: its name, its size in bytes, and the decimal places it shows (as the terminal UI's `file_size`). */
const UNITS: [name: string, size: number, places: number][] = [['KB', 1e3, 0], ['MB', 1e6, 1], ['GB', 1e9, 2], ['TB', 1e12, 2], ['PB', 1e15, 2]]

export function byteCount(bytes: number) {
  if (bytes <= 0) return 'Zero KB'
  if (bytes < 1000) return `${bytes} ${bytes === 1 ? 'byte' : 'bytes'}`
  // The size in the smallest place a unit shows, rounded half up from the whole bytes: a scaled float can sit just
  // under a half (2.55 is 2.5499… as a double) and round down where Apple's formatter rounds up.
  const shown = ([, size, places]: (typeof UNITS)[number]) => {
    const step = size / 10 ** places
    return Math.floor((bytes + step / 2) / step)
  }
  // A size that rounds to 1000 of a unit reads as 1 of the next.
  const unit = UNITS.find((u) => shown(u) < 1000 * 10 ** u[2]) ?? UNITS[UNITS.length - 1]
  return `${shown(unit) / 10 ** unit[2]} ${unit[0]}`
}

/** Seconds as `m:ss` (hours as `h:mm:ss`), for a sound's caption. */
export function clock(seconds: number) {
  const total = Math.floor(seconds)
  const h = Math.floor(total / 3600)
  const m = Math.floor((total % 3600) / 60)
  const s = total % 60
  const ss = String(s).padStart(2, '0')
  return h > 0 ? `${h}:${String(m).padStart(2, '0')}:${ss}` : `${m}:${ss}`
}

/** What the download button does now (also the Library's file rows). */
export function downloadAction(state: DownloadState | undefined) {
  return state?.kind === 'downloading' ? 'Cancel download' : state?.kind === 'failed' ? 'Retry download' : state?.kind === 'saved' ? 'Save again' : 'Download'
}

/** The line under a shared file's name: its size, the download's progress, or what went wrong. */
export function downloadDetail(state: DownloadState | undefined, file: SharedFile) {
  switch (state?.kind) {
    case 'downloading': return `${byteCount(state.received)} of ${byteCount(file.size)}`
    case 'failed': return state.message
    case 'saved': return `${byteCount(file.size)} · Saved`
    default: return byteCount(file.size)
  }
}

/** A card's caption: a voice note that couldn't play, then the download's, then the sound's clock, then the file's. */
export function cardDetail(state: DownloadState | undefined, file: SharedFile, audio?: AudioSnapshot) {
  if (audio?.phase === 'failed') return `Couldn't play. Click to retry.`
  if (state?.kind === 'downloading' || state?.kind === 'failed') return downloadDetail(state, file)
  if (audio && audio.duration > 0) return `${clock(audio.elapsed)} / ${clock(audio.duration)}`
  return downloadDetail(state, file)
}
