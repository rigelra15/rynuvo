import { describe, expect, it } from 'vitest'
import { activeLines, clock, withInterludes, lyricAt, mayControl, parseCommand, playQuery, positionMs, type MusicState, wordFill } from './music'

describe('music commands', () => {
  it('play aliases take the rest as the query', () => {
    expect(parseCommand('!p LANY Stuck')).toEqual({ kind: 'play', query: 'LANY Stuck' })
    expect(parseCommand('!PLAY  keshi  ')).toEqual({ kind: 'play', query: 'keshi' })
    expect(parseCommand('m!play Stuck')).toEqual({ kind: 'play', query: 'Stuck' })
    expect(parseCommand('m!p Stuck')).toEqual({ kind: 'play', query: 'Stuck' })
    expect(parseCommand('!p')).toEqual({ kind: 'play', query: '' })
  })
  it('loop takes an optional mode', () => {
    expect(parseCommand('!loop')).toEqual({ kind: 'cmd', cmd: { op: 'repeat', mode: null } })
    expect(parseCommand('!repeat queue')).toEqual({ kind: 'cmd', cmd: { op: 'repeat', mode: 'queue' } })
    expect(parseCommand('!loop Song')).toEqual({ kind: 'cmd', cmd: { op: 'repeat', mode: 'song' } })
  })
  it('a play command being typed shows its search once the space is in', () => {
    expect(playQuery('!p')).toBeNull()
    expect(playQuery('!p ')).toBe('')
    expect(playQuery('!play stuck la')).toBe('stuck la')
    expect(playQuery('M!P keshi')).toBe('keshi')
    expect(playQuery('!pause')).toBeNull()
    expect(playQuery('hey !p x')).toBeNull()
    expect(playQuery('!p two\nlines')).toBeNull()
  })
  it('controls', () => {
    expect(parseCommand('!skip')).toEqual({ kind: 'cmd', cmd: { op: 'skip' } })
    expect(parseCommand('!next')).toEqual({ kind: 'cmd', cmd: { op: 'skip' } })
    expect(parseCommand('!pause')).toEqual({ kind: 'cmd', cmd: { op: 'pause' } })
    expect(parseCommand('!resume')).toEqual({ kind: 'cmd', cmd: { op: 'resume' } })
    expect(parseCommand('!stop')).toEqual({ kind: 'cmd', cmd: { op: 'stop' } })
    expect(parseCommand('!q')).toEqual({ kind: 'cmd', cmd: { op: 'queue' } })
    expect(parseCommand('!queue')).toEqual({ kind: 'cmd', cmd: { op: 'queue' } })
    expect(parseCommand('!np')).toEqual({ kind: 'cmd', cmd: { op: 'nowPlaying' } })
    expect(parseCommand('!remove 2')).toEqual({ kind: 'cmd', cmd: { op: 'remove', index: 2 } })
  })
  it('anything else is a normal message', () => {
    expect(parseCommand('hello !p')).toBeNull()
    expect(parseCommand('!party tonight')).toBeNull()
    expect(parseCommand('!remove')).toBeNull()
    expect(parseCommand('!remove x')).toBeNull()
    expect(parseCommand('!')).toBeNull()
  })
})

const entry = { id: 1, videoId: 'a', title: 'Stuck', artist: 'LANY', album: null, artUrl: '', durationMs: 180_000, requestedBy: 2 }
const st = (o: Partial<MusicState>): MusicState => ({ channelId: 5, now: entry, startedAt: 10_000, pausedAt: null, queue: [], serverTime: 12_000, ...o })

describe('music position', () => {
  it('follows the server clock from when the state arrived', () => {
    expect(positionMs(st({}), 500, 500)).toBe(2_000)
    expect(positionMs(st({}), 500, 1_500)).toBe(3_000)
    expect(positionMs(st({}), 0, 1e9)).toBe(180_000)
    expect(positionMs(st({ pausedAt: 4_321 }), 0, 99_999)).toBe(4_321)
    expect(positionMs(st({ now: null }), 0, 5)).toBe(0)
  })
  it('clock text', () => {
    expect(clock(0)).toBe('0:00')
    expect(clock(76_000)).toBe('1:16')
    expect(clock(3_723_000)).toBe('1:02:03')
  })
})

describe('who may control', () => {
  it('the requester, the admin, or anyone once the requester left voice', () => {
    expect(mayControl(2, { id: 2, admin: false }, [2, 3])).toBe(true)
    expect(mayControl(2, { id: 3, admin: false }, [2, 3])).toBe(false)
    expect(mayControl(2, { id: 3, admin: true }, [2, 3])).toBe(true)
    expect(mayControl(2, { id: 3, admin: false }, [3])).toBe(true)
  })
})

describe('lyrics', () => {
  const lines = [{ ms: 1000, text: 'a', words: [] }, { ms: 3000, text: 'b', words: [] }, { ms: 5000, text: 'c', words: [] }]
  it('current line', () => {
    expect(lyricAt(lines, 0)).toBe(-1)
    expect(lyricAt(lines, 1000)).toBe(0)
    expect(lyricAt(lines, 4999)).toBe(1)
    expect(lyricAt(lines, 99_000)).toBe(2)
    expect(lyricAt([], 5)).toBe(-1)
  })
})

describe('wordFill', () => {
  const line = { ms: 1000, text: 'a b c', words: [{ ms: 1000, text: 'a' }, { ms: 1400, text: 'b' }, { ms: 2000, text: 'c' }] }
  it('fills each word over its own time', () => {
    expect(wordFill(line, 0, 900, 3000)).toBe(0)
    expect(wordFill(line, 0, 1200, 3000)).toBeCloseTo(0.5)
    expect(wordFill(line, 0, 1500, 3000)).toBe(1)
    expect(wordFill(line, 1, 1700, 3000)).toBeCloseTo(0.5)
  })
  it('lets the last word run to the next line, at most 1.5 s', () => {
    expect(wordFill(line, 2, 2500, 3000)).toBeCloseTo(0.5)
    expect(wordFill(line, 2, 2750, 99_000)).toBeCloseTo(0.5)
  })
})

describe('lines sung together', () => {
  const L = (ms: number, end?: number) => ({ ms, end, text: 'x', words: [] })
  const lines = [L(1000, 4000), L(2000, 5000), L(6000), L(9000)]
  it('every line still being sung is active, the newest included', () => {
    expect(activeLines(lines, 500)).toEqual([])
    expect(activeLines(lines, 1500)).toEqual([0])
    expect(activeLines(lines, 3000)).toEqual([0, 1])
    expect(activeLines(lines, 4500)).toEqual([1])
  })
  it('without an end time a line lasts until the next one starts', () => {
    expect(activeLines(lines, 5500)).toEqual([1])
    expect(activeLines(lines, 7000)).toEqual([2])
    expect(activeLines(lines, 99000)).toEqual([3])
  })
})

describe('interludes', () => {
  const L = (ms: number, end?: number) => ({ ms, end, text: 'x', words: [] })
  it('the wait before the first line gets dots', () => {
    const out = withInterludes([L(12000, 15000), L(15000)])
    expect(out[0]).toMatchObject({ ms: 0, end: 12000, gap: true })
    expect(out.length).toBe(3)
  })
  it('a long break between lines gets dots, a short one does not', () => {
    const out = withInterludes([L(1000, 4000), L(5000, 8000), L(20000)])
    expect(out.map((l) => !!l.gap)).toEqual([false, false, true, false])
    expect(out[2]).toMatchObject({ ms: 8000, end: 20000 })
  })
  it('lines without end times only get the opening dots', () => {
    expect(withInterludes([L(500), L(30000)]).some((l) => l.gap)).toBe(false)
  })
})
