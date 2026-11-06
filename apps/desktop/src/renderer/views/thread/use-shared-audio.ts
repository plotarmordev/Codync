import { useEffect, useState } from 'react'
import type { SharedFile } from '@shared/models'
import { useStore } from '../../store/context'
import { SharedAudio, type Sound } from './shared-audio'

/** A voice note's player for its card: fetched once from the computer, stopped when the card goes. */
export function useSharedAudio(file: SharedFile, entryId: string) {
  const store = useStore()
  const [playing, setPlaying] = useState(false)
  const [audio] = useState(() => new SharedAudio({
    load: () => store.sharedFileData(file, entryId),
    open: (data, changed) => audioElement(data, changed),
    failed: () => store.setError(`Couldn't play ${file.name}.`),
  }, setPlaying))
  useEffect(() => () => audio.dispose(), [audio])
  return { playing, toggle: () => void audio.toggle() }
}

function audioElement(data: Uint8Array, changed: (playing: boolean) => void): Sound {
  const player = new Audio(URL.createObjectURL(new Blob([data as BlobPart])))
  player.onplay = () => changed(true)
  player.onpause = () => changed(false)
  return {
    play: () => player.play(),
    pause: () => player.pause(),
    dispose: () => URL.revokeObjectURL(player.src),
  }
}
