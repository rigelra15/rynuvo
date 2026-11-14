import type { Id, WsServerMsg } from './types'

export type CodecName = 'h264' | 'hevc' | 'av1'
export interface StreamInfo { userId: Id; channelId: Id; codecs: string[]; preset: string }
export interface Source { id: string; kind: 'screen' | 'window'; name: string; app: string | null }

export type StreamEvent =
  | { kind: 'live'; viewers: Id[]; codec: string; kbps: number; hardware: boolean; relay: boolean }
  | { kind: 'stats'; userId: Id; fps: number; codec: string; kbps: number; path: 'direct' | 'relay' | 'connecting' }
  | { kind: 'ended'; userId: Id; reason: string }
  | { kind: 'error'; message: string }

export const PRESETS = [
  { id: '720p30', label: '720p 30 fps' },
  { id: '1080p60', label: '1080p 60 fps' },
  { id: '1440p60', label: '1440p 60 fps' },
  { id: 'native', label: 'Native resolution, 60 fps' },
] as const

const CODEC_BY_BYTE: Record<number, CodecName> = { 1: 'h264', 2: 'hevc', 3: 'av1' }
const RANK: Record<CodecName, number> = { h264: 1, hevc: 2, av1: 3 }
const LABEL: Record<string, string> = { h264: 'H.264', hevc: 'HEVC', av1: 'AV1' }

/** WebCodecs codec strings for the Annex B streams the app sends. */
export const CODEC_STRING: Record<CodecName, string> = {
  h264: 'avc1.640033',
  hevc: 'hvc1.1.6.L153.B0',
  av1: 'av01.0.08M.08',
}

/** Live streams, after one server message. Returns the same array when nothing changed. */
export function applyStreams(list: StreamInfo[], msg: WsServerMsg): StreamInfo[] {
  switch (msg.t) {
    case 'ready':
      return msg.d.streams ?? []
    case 'streamStarted':
      return [...list.filter((s) => s.userId !== msg.d.userId), msg.d]
    case 'streamStopped':
      return list.filter((s) => s.userId !== msg.d.userId)
    case 'voiceMemberLeft':
      return list.some((s) => s.userId === msg.d.userId) ? list.filter((s) => s.userId !== msg.d.userId) : list
    case 'userRemoved':
      return list.filter((s) => s.userId !== msg.d.id)
    default:
      return list
  }
}

/** Decodable codecs, best first; H.264 is always in the list. */
export function codecOrder(decodable: string[]): CodecName[] {
  const set = new Set<CodecName>(['h264'])
  for (const c of decodable) if (c in RANK) set.add(c as CodecName)
  return [...set].sort((a, b) => RANK[b] - RANK[a])
}

/** What this webview can decode in hardware or software. */
export async function decodableCodecs(): Promise<CodecName[]> {
  const out: string[] = []
  if (typeof VideoDecoder === 'undefined') return ['h264']
  for (const c of Object.keys(CODEC_STRING) as CodecName[]) {
    try {
      const r = await VideoDecoder.isConfigSupported({ codec: CODEC_STRING[c], optimizeForLatency: true })
      if (r.supported) out.push(c)
    } catch {
      // unsupported
    }
  }
  return codecOrder(out)
}

export interface Au { codec: CodecName; key: boolean; ts: number; data: Uint8Array }

/** One access unit from the backend: [codec u8][key u8][ts u32 BE][Annex B]. */
export function parseAu(buf: ArrayBuffer): Au | null {
  const b = new Uint8Array(buf)
  if (b.length < 7) return null
  const codec = CODEC_BY_BYTE[b[0]]
  if (!codec) return null
  const ts = ((b[2] << 24) | (b[3] << 16) | (b[4] << 8) | b[5]) >>> 0
  return { codec, key: b[1] === 1, ts, data: b.subarray(6) }
}

export interface Stats { width: number; height: number; fps: number; codec: string; path: string }

export function statsLine(s: Stats): string {
  if (s.path === 'connecting' || !s.width) return 'Connecting…'
  const via = s.path === 'direct' ? 'P2P' : 'via server'
  return `${s.width}×${s.height} · ${Math.round(s.fps)} fps · ${LABEL[s.codec] ?? s.codec} · ${via}`
}

/** Most frames queued in the decoder before deltas are dropped (latency over completeness). */
export const MAX_DECODE_QUEUE = 3
const KEY_ASK_EVERY_MS = 500

/**
 * Decides which access units reach the decoder. A delta is only decodable when every frame since
 * the last key frame was decoded, so any drop or decoder error skips to the next key frame and asks
 * the streamer for one.
 */
export class KeyGate {
  private needKey = true
  private lastAsk = -Infinity

  constructor(
    private ask: () => void,
    private now: () => number = () => performance.now(),
  ) {}

  private request() {
    this.needKey = true
    const t = this.now()
    if (t - this.lastAsk >= KEY_ASK_EVERY_MS) {
      this.lastAsk = t
      this.ask()
    }
  }

  /** True when `au` should be decoded; `queued` is the decoder's current queue size. */
  accept(au: Au, queued: number): boolean {
    if (au.key) {
      this.needKey = false
      return true
    }
    if (this.needKey) {
      if (this.lastAsk !== -Infinity) this.request()
      return false
    }
    if (queued > MAX_DECODE_QUEUE) {
      this.request()
      return false
    }
    return true
  }

  /** The decoder failed: start again from a key frame. */
  broken() {
    this.request()
  }
}
