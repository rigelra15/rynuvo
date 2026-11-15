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
  /** A music video: the player shows it above the title. */
  video?: boolean
}

export interface MusicState {
  channelId: Id
  now: MusicEntry | null
  startedAt: number
  pausedAt: number | null
  queue: MusicEntry[]
  serverTime: number
  /** The channel's lyrics source (an id from LYRICS_SOURCES); missing from servers before 0.5.1. */
  lyrics?: string
  /** Loop mode; missing from servers before 0.5.1. */
  repeat?: Repeat
  /** Votes to skip the playing song, and how many it takes (0 = no vote going). */
  skipVotes?: Id[]
  skipNeeded?: number
}

export type Repeat = 'off' | 'song' | 'queue'

/** Lyrics sources a voice channel can pick, as the server knows them. */
export const LYRICS_SOURCES = [
  { id: 'auto', name: 'Auto' },
  { id: 'betterlyrics', name: 'BetterLyrics' },
  { id: 'lrcred', name: 'LrcRed' },
  { id: 'simpmusic', name: 'SimpMusic' },
  { id: 'lrclib', name: 'LRCLIB' },
  { id: 'netease', name: 'NetEase' },
] as const

/** A song found on YouTube Music, before it is queued. */
export interface SongResult {
  videoId: string
  title: string
  artist: string
  album: string | null
  artUrl: string
  durationMs: number
  video?: boolean
}

export type MusicCmd =
  | { op: 'add'; entry: SongResult }
  | { op: 'lyrics'; source: string }
  | { op: 'repeat'; mode: Repeat | null }
  | { op: 'skip' }
  | { op: 'pause' }
  | { op: 'resume' }
  | { op: 'seek'; ms: number }
  | { op: 'remove'; index: number }
  | { op: 'stop' }
  | { op: 'queue' }
  | { op: 'nowPlaying' }

export interface Word { ms: number; text: string }
export interface LyricLine {
  ms: number
  /** When the line ends, if the source says; lines may overlap (duets, backing vocals). */
  end?: number | null
  text: string
  words: Word[]
  /** Backing vocals sung with the line. */
  bg?: Word[]
  /** Sung by the duet's second voice. */
  duet?: boolean
  /** No one sings here: the breathing dots of an intro or a break. */
  gap?: boolean
}
export interface Lyrics { lines: LyricLine[]; plain: string | null; source: string }

/** How well a source knows the song, for the lyrics source menu. */
export function lyricsStatus(l: Lyrics | null | undefined): { label: string; tone: 'good' | 'meh' | 'bad' | 'wait' } {
  if (l === undefined) return { label: 'Checking…', tone: 'wait' }
  if (l === null) return { label: 'Not found', tone: 'bad' }
  if (l.lines.some((x) => x.words.length)) return { label: 'Word synced', tone: 'good' }
  if (l.lines.length) return { label: 'Line synced', tone: 'good' }
  return { label: 'Plain text', tone: 'meh' }
}

/** A lyric line's English translation (when not English) and romanization (when not in Latin letters). */
export interface Annotation { translation: string | null; roman: string | null }

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
  if (word === '!loop' || word === '!repeat') return { kind: 'cmd', cmd: { op: 'repeat', mode: (rest.toLowerCase() || null) as Repeat | null } }
  if (word === '!remove' && /^\d+$/.test(rest)) return { kind: 'cmd', cmd: { op: 'remove', index: Number(rest) } }
  return null
}

/** The search typed so far after a play command ("!p stuck" → "stuck"), once its space is typed;
 *  null when the text isn't a play command. */
export function playQuery(text: string): string | null {
  const m = /^(\S+) (.*)$/.exec(text.trimStart())
  return m && PLAY.has(m[1].toLowerCase()) ? m[2] : null
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

/** Indexes of the lines being sung at `ms`: the newest line started, plus any earlier line whose end
 *  hasn't come yet (a duet partner or backing line still going). A line without an end lasts until
 *  the next one starts. */
export function activeLines(lines: LyricLine[], ms: number): number[] {
  const last = lyricAt(lines, ms)
  if (last < 0) return []
  const out: number[] = []
  for (let i = 0; i < last; i++) {
    const end = lines[i].end ?? lines[i + 1].ms
    if (ms < end) out.push(i)
  }
  out.push(last)
  return out
}

const OPENING_MS = 3000
const BREAK_MS = 5000

/** `lines` with dots for the wait before the first line and for long breaks (when the source says
 *  when lines end), the way Apple Music shows them. */
export function withInterludes(lines: LyricLine[]): LyricLine[] {
  const out: LyricLine[] = []
  if (lines.length && lines[0].ms >= OPENING_MS) out.push({ ms: 0, end: lines[0].ms, text: '', words: [], gap: true })
  lines.forEach((l, i) => {
    out.push(l)
    const next = lines[i + 1]
    if (next && l.end != null && next.ms - l.end >= BREAK_MS) out.push({ ms: l.end, end: next.ms, text: '', words: [], gap: true })
  })
  return out
}

/** How far (0–1) word `j` of `line` has been sung at `ms`; the last word runs until the next line
 *  starts (`nextMs`), but no longer than 1.5 s. */
export function wordFill(line: LyricLine, j: number, ms: number, nextMs: number): number {
  const w = line.words[j]
  const end = line.words[j + 1]?.ms ?? Math.min(nextMs, w.ms + 1500)
  if (end <= w.ms) return ms >= w.ms ? 1 : 0
  return Math.max(0, Math.min(1, (ms - w.ms) / (end - w.ms)))
}
