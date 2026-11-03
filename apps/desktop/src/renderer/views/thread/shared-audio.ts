/** A loaded sound, ready to play (an `HTMLAudioElement` in the app). */
export interface Sound {
  play(): Promise<void>
  pause(): void
  dispose(): void
}

/** Where a card's player gets its sound and says what happens. */
export interface SharedAudioPlatform {
  /** The sound's bytes, fetched once from the computer; null when they can't be read. */
  load(): Promise<Uint8Array | null>
  /** A player for the bytes, reporting each play and pause. */
  open(data: Uint8Array, playing: (playing: boolean) => void): Sound
  failed(): void
}

/** Plays a sound a bot shared (a voice note); one card's player, disposed when the card goes. */
export class SharedAudio {
  private sounding = false
  get playing() { return this.sounding }
  private sound: Sound | null = null
  // A tap while the sound loads is dropped, so it never starts a second player.
  private loading = false
  private disposed = false
  private readonly platform: SharedAudioPlatform
  private readonly changed: (playing: boolean) => void

  constructor(platform: SharedAudioPlatform, changed: (playing: boolean) => void) {
    this.platform = platform
    this.changed = changed
  }

  /** Settles once this tap's pause, play or failure is done. */
  async toggle() {
    if (this.playing) return this.sound?.pause()
    if (this.loading) return
    this.loading = true
    try {
      await this.play()
    } catch {
      this.platform.failed()
    } finally {
      this.loading = false
    }
  }

  dispose() {
    this.disposed = true
    this.sound?.pause()
    this.sound?.dispose()
  }

  private async play() {
    if (!this.sound) {
      const data = await this.platform.load()
      // The card went while the sound loaded: nothing could pause it any more.
      if (this.disposed) return
      if (!data) throw new Error('unreadable')
      this.sound = this.platform.open(data, (playing) => {
        this.sounding = playing
        this.changed(playing)
      })
    }
    await this.sound.play()
  }
}
