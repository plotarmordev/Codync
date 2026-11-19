/** One voice-note card's player state. */
export type AudioPhase = 'idle' | 'loading' | 'playing' | 'paused' | 'failed'
/** Which step of a play failed: fetching or opening the sound, or starting it. */
export type AudioFailure = 'load' | 'playback'

/** What a card shows: the player's phase and the sound's clock (seconds; duration 0 while unknown). */
export interface AudioSnapshot {
  phase: AudioPhase
  elapsed: number
  duration: number
}

/** A loaded sound, ready to play (an `HTMLAudioElement` in the app). */
export interface Sound {
  play(): Promise<void>
  pause(): void
  /** Moves its playhead (seconds), playing or paused. */
  seek(seconds: number): void
  dispose(): void
}

/** What a sound reports back to its card's player. */
export interface SoundEvents {
  /** It started or stopped playing. */
  playing(playing: boolean): void
  /** Its clock moved or its length became known (seconds). */
  time(elapsed: number, duration: number): void
  /** It played to the end. */
  ended(): void
  /** It broke while playing (after `play()` already settled). */
  failed(): void
}

/** Where a card's player gets its sound. */
export interface SharedAudioPlatform {
  /** The sound's bytes, fetched once from the computer; null when they can't be read. */
  load(): Promise<Uint8Array | null>
  /** A player for the bytes. */
  open(data: Uint8Array, events: SoundEvents): Sound
  /** The bytes' length (seconds) from their headers, without playing them; 0 when they don't say. */
  length(data: Uint8Array): Promise<number>
}

/** Plays a sound a bot shared (a voice note); one card's player, disposed when the card goes. */
export class SharedAudio {
  private phase: AudioPhase = 'idle'
  private elapsed = 0
  private duration: number
  private sound: Sound | null = null
  private disposed = false
  /** The step this tap is on, so a failure says which one broke. */
  failure: AudioFailure = 'load'
  private readonly platform: SharedAudioPlatform
  private readonly changed: (snapshot: AudioSnapshot) => void
  /** The length the host or the card read from the sound's headers (0 when unknown), shown until the sound itself says. */
  private known: number
  /** The sound's bytes, one fetch shared by the preload, the length read and a tap; a failed one is forgotten, so
   *  the next call fetches again. */
  private bytes: Promise<Uint8Array | null> | null = null

  constructor(platform: SharedAudioPlatform, changed: (snapshot: AudioSnapshot) => void, known: number) {
    this.platform = platform
    this.changed = changed
    this.known = known
    this.duration = known
  }

  get snapshot(): AudioSnapshot {
    return { phase: this.phase, elapsed: this.elapsed, duration: this.duration }
  }

  /** Settles once this tap's pause, play or failure is done. A tap while the card's preload runs shows the spinner
   *  and plays once those bytes arrive. */
  async toggle() {
    if (this.phase === 'playing') return this.sound?.pause()
    // A second tap while the sound loads is dropped, so it never starts a second player.
    if (this.phase === 'loading' || this.disposed) return
    this.setPhase('loading')
    this.failure = 'load'
    try {
      await this.play()
    } catch (error) {
      // A pause before play() settled rejects it with AbortError: the sound stays loaded where it was.
      if (error instanceof DOMException && error.name === 'AbortError') return
      // play() may already have started the sound before its promise rejects: the card must not stay on pause.
      this.drop()
      this.setPhase('failed')
    }
  }

  dispose() {
    this.disposed = true
    this.drop()
    this.phase = 'idle'
  }

  /** The host's length arriving after the card mounted (a backfilled note): shown until the sound itself says. */
  setKnownDuration(seconds: number) {
    if (seconds <= 0 || this.duration > 0) return
    this.known = seconds
    this.duration = seconds
    this.changed(this.snapshot)
  }

  /** Fetches the sound as the card shows, so a tap plays at once; a note shared with no length (an older computer)
   *  reads it from those same bytes, so the clock is there before the first play. A fetch or read that fails stays
   *  quiet: the tap retries and reports it. */
  async preload() {
    if (this.disposed) return
    const data = await this.data()
    if (!data || this.disposed || this.duration > 0) return
    const seconds = await this.platform.length(data)
    if (!this.disposed) this.setKnownDuration(seconds)
  }

  /** Moves a loaded sound's playhead, held to its length; playing or paused, it stays so. */
  seek(seconds: number) {
    if (!this.sound || this.duration <= 0) return
    this.elapsed = Math.min(Math.max(seconds, 0), this.duration)
    this.sound.seek(this.elapsed)
    this.changed(this.snapshot)
  }

  private async play() {
    if (!this.sound) {
      const data = await this.data()
      // The card went while the sound loaded: nothing could pause it any more.
      if (this.disposed) return
      if (!data) throw new Error('unreadable')
      const sound = this.platform.open(data, {
        playing: (on) => {
          if (this.sound !== sound) return
          this.setPhase(on ? 'playing' : this.phase === 'idle' ? 'idle' : 'paused')
        },
        time: (elapsed, duration) => {
          if (this.sound !== sound) return
          this.elapsed = elapsed
          if (duration > 0) this.duration = duration
          this.changed(this.snapshot)
        },
        ended: () => {
          if (this.sound !== sound) return
          this.elapsed = 0
          this.setPhase('idle')
        },
        failed: () => {
          if (this.sound !== sound) return
          this.drop()
          this.setPhase('failed')
        },
      })
      this.sound = sound
    }
    this.failure = 'playback'
    await this.sound.play()
  }

  private data() {
    this.bytes ??= this.platform.load().then((data) => {
      if (!data) this.bytes = null
      return data
    })
    return this.bytes
  }

  /** Drops the player and its clock, so a failure shows no stale progress and the next tap fetches and decodes again. */
  private drop() {
    const sound = this.sound
    this.bytes = null
    this.sound = null
    this.elapsed = 0
    this.duration = this.known
    sound?.pause()
    sound?.dispose()
  }

  private setPhase(phase: AudioPhase) {
    this.phase = phase
    this.changed(this.snapshot)
  }
}
