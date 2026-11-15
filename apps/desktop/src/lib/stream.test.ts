import { describe, expect, it, test } from 'vitest'
import { applyStreams, codecOrder, KeyGate, parseAu, selfCheckLines, spsSize, statsLine, type Au, type StreamInfo } from './stream'

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

describe('spsSize', () => {
  // Real parameter sets: VideoToolbox (what the Mac app encodes with), x264 and x265.
  const au = (hex: string) => Uint8Array.from([0, 0, 0, 1, ...hex.match(/../g)!.map((b) => parseInt(b, 16)), 0, 0, 1, 0x65, 0x88])
  it.each([
    ['h264', '27640032ac5680780277e9', 1920, 1246],
    ['h264', '27640033ac56802f40f7fb40', 3024, 1964],
    ['h264', '67640028acd940780227a970110000030001000003003c0f183196', 1918, 1080],
    ['hevc', '420101016000000300b00000030000030096a003c0801387d6205ee45914bff2e7f13fac05a810101004', 1920, 1246],
    ['hevc', '420101016000000300b00000030000030096a0017a2007b1f78817b916452ffcb9fc4feb016a04040401', 3024, 1964],
    ['hevc', '4201010408000003009e0800000300007890007810021cefcb2b349265780b7020200040000003004000000782', 1918, 1080],
  ] as const)('reads the picture size from a %s key frame (%s)', (codec, sps, width, height) => {
    expect(spsSize(codec, au(sps))).toEqual({ width, height })
  })

  it('finds nothing in a frame without a parameter set', () => {
    expect(spsSize('h264', Uint8Array.from([0, 0, 0, 1, 0x41, 0x9a, 0x02]))).toBeNull()
    expect(spsSize('av1', Uint8Array.from([0x12, 0x00]))).toBeNull()
  })
})
