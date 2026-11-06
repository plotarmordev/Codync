import type { SharedFile } from '@shared/models'
import type { DownloadState } from '../../store/file-downloads'
import type { AudioSnapshot } from './shared-audio'

/** A byte count the way Apple's `ByteCountFormatter(.file)` writes it: decimal units, no needless zeros. */
export function byteCount(bytes: number) {
  if (bytes <= 0) return 'Zero KB'
  if (bytes < 1000) return `${bytes} ${bytes === 1 ? 'byte' : 'bytes'}`
  const units = ['KB', 'MB', 'GB']
  const decimals = [0, 1, 2]
  let value = bytes / 1000
  let unit = 0
  while (unit < units.length - 1 && Number(value.toFixed(decimals[unit])) >= 1000) {
    value /= 1000
    unit++
  }
  return `${parseFloat(value.toFixed(decimals[unit]))} ${units[unit]}`
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
