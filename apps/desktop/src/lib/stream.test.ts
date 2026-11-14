import { describe, expect, test } from 'vitest'
import { applyStreams, codecOrder, KeyGate, parseAu, selfCheckLines, statsLine, type Au, type StreamInfo } from './stream'

const s = (userId: number, channelId = 10): StreamInfo => ({ userId, channelId, codecs: ['h264'], preset: '1080p60' })

describe('applyStreams', () => {
  test('ready, start and stop', () => {
    let list = applyStreams([], { t: 'ready', d: { me: { id: 1, nickname: 'a', isAdmin: false }, users: [], channels: [], online: [], alerts: [], streams: [s(2)] } })
    expect(list).toEqual([s(2)])
    list = applyStreams(list, { t: 'streamStarted', d: s(3) })
    list = applyStreams(list, { t: 'streamStarted', d: { ...s(3), preset: '720p30' } })
    expect(list.map((x) => x.userId)).toEqual([2, 3])
    expect(list[1].preset).toBe('720p30')
    list = applyStreams(list, { t: 'streamStopped', d: { userId: 2, channelId: 10 } })
    expect(list.map((x) => x.userId)).toEqual([3])
  })

  test('a streamer leaving voice ends the stream', () => {
    const list = applyStreams([s(2)], { t: 'voiceMemberLeft', d: { userId: 2, channelId: 10 } })
    expect(list).toEqual([])
  })
})

describe('codecOrder', () => {
  test('prefers HEVC over H.264 when both decode, AV1 above both', () => {
    expect(codecOrder(['h264', 'hevc'])).toEqual(['hevc', 'h264'])
    expect(codecOrder(['h264', 'av1', 'hevc'])).toEqual(['av1', 'hevc', 'h264'])
  })
  test('H.264 is always offered', () => {
    expect(codecOrder([])).toEqual(['h264'])
  })
})

describe('parseAu', () => {
  test('reads the 6-byte header', () => {
    const b = new Uint8Array([2, 1, 0, 0, 1, 0, 0, 0, 0, 1, 0x40])
    const au = parseAu(b.buffer)!
    expect(au.codec).toBe('hevc')
    expect(au.key).toBe(true)
    expect(au.ts).toBe(256)
    expect([...au.data]).toEqual([0, 0, 0, 1, 0x40])
  })
  test('rejects short or unknown input', () => {
    expect(parseAu(new Uint8Array([1, 0]).buffer)).toBeNull()
    expect(parseAu(new Uint8Array([9, 0, 0, 0, 0, 0, 1]).buffer)).toBeNull()
  })
})

test('statsLine', () => {
  expect(statsLine({ width: 1920, height: 1080, fps: 59.6, codec: 'h264', path: 'direct' })).toBe('1920×1080 · 60 fps · H.264 · P2P')
  expect(statsLine({ width: 1280, height: 720, fps: 30, codec: 'hevc', path: 'relay' })).toBe('1280×720 · 30 fps · HEVC · via server')
  expect(statsLine({ width: 0, height: 0, fps: 0, codec: '', path: 'connecting' })).toBe('Connecting…')
})

describe('KeyGate', () => {
  const au = (key: boolean): Au => ({ codec: 'h264', key, ts: 0, data: new Uint8Array([0, 0, 0, 1]) })

  test('waits for a key frame, then decodes deltas in order', () => {
    const asks: number[] = []
    const g = new KeyGate(() => asks.push(1), () => 0)
    expect(g.accept(au(false), 0)).toBe(false)
    expect(g.accept(au(true), 0)).toBe(true)
    expect(g.accept(au(false), 0)).toBe(true)
  })

  test('a dropped delta breaks the chain: skip until the next key frame and ask for one', () => {
    let now = 0
    const asks: number[] = []
    const g = new KeyGate(() => asks.push(now), () => now)
    g.accept(au(true), 0)
    expect(g.accept(au(false), 10)).toBe(false) // decoder backed up
    expect(asks).toEqual([0])
    now = 100
    expect(g.accept(au(false), 0)).toBe(false) // not decodable without its reference
    expect(asks).toEqual([0]) // asked recently
    now = 600
    g.accept(au(false), 0)
    expect(asks).toEqual([0, 600])
    expect(g.accept(au(true), 0)).toBe(true)
    expect(g.accept(au(false), 0)).toBe(true)
  })

  test('a decoder error asks for a key frame', () => {
    const asks: number[] = []
    const g = new KeyGate(() => asks.push(1), () => 0)
    g.accept(au(true), 0)
    g.broken()
    expect(asks).toHaveLength(1)
    expect(g.accept(au(false), 0)).toBe(false)
  })
})

describe('selfCheckLines', () => {
  test('hardware encoder and permission granted', () => {
    const l = selfCheckLines({ encoders: [{ codec: 'hevc', hardware: true, works: true }, { codec: 'h264', hardware: false, works: true }], capture: 'granted' })
    expect(l).toEqual([
      { label: 'HEVC encoder', ok: true, detail: 'Hardware' },
      { label: 'H.264 encoder', ok: true, detail: 'Software (uses more CPU)' },
      { label: 'Screen capture', ok: true, detail: 'Allowed' },
    ])
  })

  test('denied capture and a broken encoder are flagged', () => {
    const l = selfCheckLines({ encoders: [{ codec: 'av1', hardware: false, works: false }], capture: 'denied' })
    expect(l[0]).toEqual({ label: 'AV1 encoder', ok: false, detail: 'Not available' })
    expect(l[1]).toEqual({ label: 'Screen capture', ok: false, detail: 'Not allowed — turn on Rynuvo in Screen Recording settings' })
  })
})
