import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import type { Id } from './types'

// Closed captions: what's said in the call (people and shared-screen sound), written out on this
// device. The Rust side downloads the model once, then sends lines as they're heard.

export type CaptionLang = 'id' | 'en'
type Event =
  | { kind: 'preparing'; step: 'downloading' | 'loading'; progress: number }
  | { kind: 'on' }
  | { kind: 'off' }
  | { kind: 'error'; message: string }
  | { kind: 'line'; speaker: Id | null; text: string; at: number }

export interface CaptionLine {
  n: number
  speaker: Id | null
  text: string
}

/** How long a line stays up once nothing new comes. */
const LINGER_MS = 7000
const KEY = 'rynuvo.captionLang'

class Captions {
  status = $state<'off' | 'downloading' | 'loading' | 'on' | 'error'>('off')
  progress = $state(0)
  error = $state<string | null>(null)
  lang = $state<CaptionLang>(load())
  lines = $state<CaptionLine[]>([])
  private n = 0
  private listening = false
  private timer: ReturnType<typeof setTimeout> | undefined

  get wanted() {
    return this.status !== 'off' && this.status !== 'error'
  }

  private async listen() {
    if (this.listening) return
    this.listening = true
    await listen<Event>('rynuvo://captions', (e) => this.apply(e.payload))
    const info = await invoke<{ on: boolean }>('captions_info')
    if (info.on) this.status = 'on'
  }

  private apply(e: Event) {
    switch (e.kind) {
      case 'preparing':
        this.status = e.step
        this.progress = e.progress
        return
      case 'on':
        this.status = 'on'
        this.error = null
        return
      case 'off':
        this.status = 'off'
        this.lines = []
        return
      case 'error':
        this.status = 'error'
        this.error = e.message
        return
      case 'line':
        if (this.status !== 'on') return
        this.lines = [...this.lines, { n: ++this.n, speaker: e.speaker, text: e.text }].slice(-2)
        clearTimeout(this.timer)
        this.timer = setTimeout(() => (this.lines = []), LINGER_MS)
        return
    }
  }

  async init() {
    try {
      await this.listen()
    } catch {
      // not in the app (tests)
    }
  }

  async toggle() {
    await this.init()
    if (this.wanted) {
      this.status = 'off'
      this.lines = []
      await invoke('captions_stop')
    } else {
      this.error = null
      this.status = 'loading'
      this.progress = 0
      await invoke('captions_start', { lang: this.lang })
    }
  }

  async setLang(l: CaptionLang) {
    this.lang = l
    try {
      localStorage.setItem(KEY, l)
    } catch {
      // not remembered
    }
    this.lines = []
    await invoke('captions_set_lang', { lang: l })
  }
}

function load(): CaptionLang {
  try {
    return localStorage.getItem(KEY) === 'en' ? 'en' : 'id'
  } catch {
    return 'id'
  }
}

export const captions = new Captions()
