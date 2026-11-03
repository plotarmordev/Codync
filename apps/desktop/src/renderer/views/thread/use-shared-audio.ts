import { useEffect, useRef, useState } from 'react'
import type { SharedFile } from '@shared/models'
import { useStore } from '../../store/context'

/** Plays a sound a bot shared (a voice note), fetched once from the computer; stops when the card goes. */
export function useSharedAudio(file: SharedFile, entryId: string) {
  const store = useStore()
  const audio = useRef<HTMLAudioElement | null>(null)
  // A tap while the sound loads is dropped, so it never starts a second player.
  const loading = useRef(false)
  const disposed = useRef(false)
  const [playing, setPlaying] = useState(false)
  useEffect(() => () => {
    disposed.current = true
    audio.current?.pause()
    if (audio.current) URL.revokeObjectURL(audio.current.src)
  }, [])
  const play = async () => {
    if (!audio.current) {
      const data = await store.sharedFileData(file, entryId)
      // The card went while the sound loaded: nothing could pause it any more.
      if (disposed.current) return
      if (!data) throw new Error('unreadable')
      const player = new Audio(URL.createObjectURL(new Blob([data as BlobPart])))
      player.onplay = () => setPlaying(true)
      player.onpause = () => setPlaying(false)
      audio.current = player
    }
    await audio.current.play()
  }
  const toggle = () => {
    if (playing) audio.current?.pause()
    else if (!loading.current) {
      loading.current = true
      play().catch(() => store.setError(`Couldn't play ${file.name}.`)).finally(() => { loading.current = false })
    }
  }
  return { playing, toggle }
}
