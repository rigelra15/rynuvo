// Listen-together music: types mirroring rynuvo-proto, chat commands, and timing helpers.
import type { Id } from './types'

export interface MusicEntry {
  id: number
  videoId: string
  title: string
  artist: string
  album: string | null
  artUrl: string
  durationMs: number
  requestedBy: Id
}

export interface MusicState {
  channelId: Id
  now: MusicEntry | null
  startedAt: number
  pausedAt: number | null
  queue: MusicEntry[]
  serverTime: number
}

export type MusicCmd =
  | { op: 'skip' }
  | { op: 'pause' }
  | { op: 'resume' }
  | { op: 'seek'; ms: number }
  | { op: 'remove'; index: number }
  | { op: 'stop' }
  | { op: 'queue' }
  | { op: 'nowPlaying' }

export interface Word { ms: number; text: string }
export interface LyricLine { ms: number; text: string; words: Word[] }
export interface Lyrics { lines: LyricLine[]; plain: string | null; source: string }

/** This app's progress fetching a song. */
export interface MusicStatus { videoId: string; status: 'loading' | 'ready' | 'error'; message: string | null }

export type Parsed = { kind: 'play'; query: string } | { kind: 'cmd'; cmd: MusicCmd }

const PLAY = new Set(['!p', '!play', 'm!p', 'm!play'])
const SIMPLE: Record<string, MusicCmd> = {
  '!skip': { op: 'skip' },
  '!next': { op: 'skip' },
  '!pause': { op: 'pause' },
  '!resume': { op: 'resume' },
  '!stop': { op: 'stop' },
  '!queue': { op: 'queue' },
  '!q': { op: 'queue' },
  '!np': { op: 'nowPlaying' },
}

/** A music command typed in chat, or null for an ordinary message. */
export function parseCommand(text: string): Parsed | null {
  const t = text.trim()
  const m = /^(\S+)(?:\s+([\s\S]*))?$/.exec(t)
  if (!m) return null
  const word = m[1].toLowerCase()
  const rest = (m[2] ?? '').trim()
  if (PLAY.has(word)) return { kind: 'play', query: rest }
  if (word in SIMPLE) return { kind: 'cmd', cmd: SIMPLE[word] }
  if (word === '!remove' && /^\d+$/.test(rest)) return { kind: 'cmd', cmd: { op: 'remove', index: Number(rest) } }
  return null
}

/** Where the song is (ms), from a state that arrived when the local clock read `receivedAt`. */
export function positionMs(s: MusicState, receivedAt: number, now: number): number {
  const dur = s.now?.durationMs ?? 0
  const at = s.pausedAt ?? now - receivedAt + s.serverTime - s.startedAt
  return Math.max(0, Math.min(dur, at))
}

export function clock(ms: number): string {
  const s = Math.floor(ms / 1000)
  const h = Math.floor(s / 3600)
  const mm = Math.floor((s % 3600) / 60)
  const ss = String(s % 60).padStart(2, '0')
  return h ? `${h}:${String(mm).padStart(2, '0')}:${ss}` : `${mm}:${ss}`
}

/** Same rule as the server: the requester, the admin, or anyone once the requester left voice. */
export function mayControl(requestedBy: Id, me: { id: Id; admin: boolean }, inVoice: Id[]): boolean {
  return me.admin || me.id === requestedBy || !inVoice.includes(requestedBy)
}

/** Index of the line being sung at `ms`, or -1 before the first. */
export function lyricAt(lines: LyricLine[], ms: number): number {
  let lo = 0
  let hi = lines.length
  while (lo < hi) {
    const mid = (lo + hi) >> 1
    if (lines[mid].ms <= ms) lo = mid + 1
    else hi = mid
  }
  return lo - 1
}
