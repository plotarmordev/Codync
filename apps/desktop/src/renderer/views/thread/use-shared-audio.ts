import { useEffect, useState } from 'react'
import type { SharedFile } from '@shared/models'
import { analytics } from '../../store/analytics'
import { useStore } from '../../store/context'
import { SharedAudio, type AudioSnapshot, type Sound, type SoundEvents } from './shared-audio'

/** A voice note's player for its card: fetched once from the computer, stopped when the card goes. Its clock starts
 *  at the length the host read, so the bar and `0:00 / 0:09` show before the first play. */
export function useSharedAudio(file: SharedFile, entryId: string) {
  const store = useStore()
  const known = (file.durationMs ?? 0) / 1000
  const [snapshot, setSnapshot] = useState<AudioSnapshot>({ phase: 'idle', elapsed: 0, duration: known })
  const [audio] = useState(() => new SharedAudio({
    load: () => store.sharedFileData(file, entryId),
    open: (data, events) => audioElement(data, events),
  }, setSnapshot, known))
  useEffect(() => () => audio.dispose(), [audio])
  return { snapshot, toggle: () => void play(audio), seek: (seconds: number) => audio.seek(seconds) }
}

/** Taps the card's control; a start from the beginning reports whether the note played, else which step broke. */
async function play(audio: SharedAudio) {
  const fromStart = audio.snapshot.phase === 'idle' || audio.snapshot.phase === 'failed'
  await audio.toggle()
  if (!fromStart) return
  const { phase } = audio.snapshot
  if (phase === 'playing') analytics.track({ event: 'voice_note_played', outcome: 'played' })
  if (phase === 'failed') analytics.track({ event: 'voice_note_played', outcome: 'failed', error: audio.failure })
}

function audioElement(data: Uint8Array, events: SoundEvents): Sound {
  const player = new Audio(URL.createObjectURL(new Blob([data as BlobPart])))
  player.onplay = () => events.playing(true)
  player.onpause = () => events.playing(false)
  player.ontimeupdate = player.ondurationchange = () => events.time(player.currentTime, Number.isFinite(player.duration) ? player.duration : 0)
  player.onended = () => events.ended()
  player.onerror = () => events.failed()
  return {
    play: () => player.play(),
    pause: () => player.pause(),
    seek: (seconds) => { player.currentTime = seconds },
    dispose: () => URL.revokeObjectURL(player.src),
  }
}
