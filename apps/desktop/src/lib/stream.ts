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

export interface SelfCheck {
  encoders: { codec: string; hardware: boolean; works: boolean }[]
  capture: 'granted' | 'denied'
}

const CODEC_LABEL: Record<string, string> = { h264: 'H.264', hevc: 'HEVC', av1: 'AV1' }

/** Settings → Screen share self-check, one line per finding. */
export function selfCheckLines(c: SelfCheck): { label: string; ok: boolean; detail: string }[] {
  const lines = c.encoders.map((e) => ({
    label: `${CODEC_LABEL[e.codec] ?? e.codec} encoder`,
    ok: e.works,
    detail: !e.works ? 'Not available' : e.hardware ? 'Hardware' : 'Software (uses more CPU)',
  }))
  lines.push(
    c.capture === 'granted'
      ? { label: 'Screen capture', ok: true, detail: 'Allowed' }
      : { label: 'Screen capture', ok: false, detail: 'Not allowed — turn on Rynuvo in Screen Recording settings' },
  )
  return lines
}

/** Reads an RBSP (emulation-prevention bytes removed) bit by bit. */
class Bits {
  private bytes: number[] = []
  private pos = 0
  constructor(nal: Uint8Array) {
    for (let i = 0; i < nal.length; i++) {
      if (i >= 2 && nal[i] === 3 && nal[i - 1] === 0 && nal[i - 2] === 0) continue
      this.bytes.push(nal[i])
    }
  }
  u(n: number): number {
    let v = 0
    for (let i = 0; i < n; i++) {
      const b = this.bytes[this.pos >> 3]
      if (b === undefined) throw new Error('short')
      v = v * 2 + ((b >> (7 - (this.pos & 7))) & 1)
      this.pos++
    }
    return v
  }
  skip(n: number) {
    this.pos += n
  }
  ue(): number {
    let zeros = 0
    while (this.u(1) === 0) if (++zeros > 31) throw new Error('bad')
    return 2 ** zeros - 1 + this.u(zeros)
  }
  se(): number {
    const k = this.ue()
    return k & 1 ? (k + 1) / 2 : -k / 2
  }
}

function nalUnits(annexb: Uint8Array): Uint8Array[] {
  const starts: number[] = []
  for (let i = 0; i + 2 < annexb.length; i++) {
    if (annexb[i] === 0 && annexb[i + 1] === 0 && annexb[i + 2] === 1) {
      starts.push(i + 3)
      i += 2
    }
  }
  return starts.map((s, k) => {
    let e = k + 1 < starts.length ? starts[k + 1] - 3 : annexb.length
    while (e > s && annexb[e - 1] === 0) e--
    return annexb.subarray(s, e)
  })
}

const HIGH_PROFILES = new Set([100, 110, 122, 244, 44, 83, 86, 118, 128, 138, 139, 134, 135])

function h264Size(nal: Uint8Array) {
  const b = new Bits(nal.subarray(1))
  const profile = b.u(8)
  b.skip(16)
  b.ue()
  let chroma = 1
  let separate = 0
  if (HIGH_PROFILES.has(profile)) {
    chroma = b.ue()
    if (chroma === 3) separate = b.u(1)
    b.ue()
    b.ue()
    b.skip(1)
    if (b.u(1)) {
      for (let i = 0; i < (chroma !== 3 ? 8 : 12); i++) {
        if (!b.u(1)) continue
        let last = 8
        let next = 8
        for (let j = 0; j < (i < 6 ? 16 : 64) && next !== 0; j++) {
          next = (last + b.se() + 256) % 256
          if (next !== 0) last = next
        }
      }
    }
  }
  b.ue()
  const poc = b.ue()
  if (poc === 0) b.ue()
  else if (poc === 1) {
    b.skip(1)
    b.se()
    b.se()
    const n = b.ue()
    for (let i = 0; i < n; i++) b.se()
  }
  b.ue()
  b.skip(1)
  const wMbs = b.ue() + 1
  const hUnits = b.ue() + 1
  const frameOnly = b.u(1)
  if (!frameOnly) b.skip(1)
  b.skip(1)
  let [l, r, t, bo] = [0, 0, 0, 0]
  if (b.u(1)) [l, r, t, bo] = [b.ue(), b.ue(), b.ue(), b.ue()]
  const mono = chroma === 0 || separate === 1
  const cx = mono ? 1 : chroma === 3 ? 1 : 2
  const cy = (mono ? 1 : chroma === 1 ? 2 : 1) * (2 - frameOnly)
  return { width: wMbs * 16 - cx * (l + r), height: (2 - frameOnly) * hUnits * 16 - cy * (t + bo) }
}

function hevcSize(nal: Uint8Array) {
  const b = new Bits(nal.subarray(2))
  b.skip(4)
  const subLayers = b.u(3)
  b.skip(1)
  b.skip(2 + 1 + 5 + 32 + 48 + 8) // general profile, tier and level
  const profileHere: number[] = []
  const levelHere: number[] = []
  for (let i = 0; i < subLayers; i++) {
    profileHere.push(b.u(1))
    levelHere.push(b.u(1))
  }
  if (subLayers > 0) b.skip(2 * (8 - subLayers))
  for (let i = 0; i < subLayers; i++) b.skip((profileHere[i] ? 88 : 0) + (levelHere[i] ? 8 : 0))
  b.ue()
  const chroma = b.ue()
  if (chroma === 3) b.skip(1)
  let width = b.ue()
  let height = b.ue()
  if (b.u(1)) {
    const [l, r, t, bo] = [b.ue(), b.ue(), b.ue(), b.ue()]
    width -= (chroma === 1 || chroma === 2 ? 2 : 1) * (l + r)
    height -= (chroma === 1 ? 2 : 1) * (t + bo)
  }
  return { width, height }
}

/**
 * The picture size in a key frame's sequence parameter set, or null when it carries none (or the
 * codec isn't one we read). WebCodecs needs it: without a coded size some decoders (WebView2 on
 * Windows) assume 1280×720 and show only that corner of a bigger picture.
 */
export function spsSize(codec: CodecName, annexb: Uint8Array): { width: number; height: number } | null {
  if (codec === 'av1') return null
  for (const nal of nalUnits(annexb)) {
    if (!nal.length) continue
    try {
      if (codec === 'h264' && (nal[0] & 0x1f) === 7) return h264Size(nal)
      if (codec === 'hevc' && ((nal[0] >> 1) & 0x3f) === 33) return hevcSize(nal)
    } catch {
      return null
    }
  }
  return null
}
