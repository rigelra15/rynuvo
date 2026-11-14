import { describe, expect, test } from 'vitest'
import { activeNow, groupTitle, presence } from './home'

const channels = [
  { id: 10, name: 'hello' },
  { id: 11, name: 'gaming' },
]
const nick = (id: number) => ({ 1: 'Rigel', 2: 'budi', 3: 'Ana', 4: 'Dimas' })[id] ?? '?'
const vm = (userId: number, channelId: number) => ({ userId, channelId, muted: false, deafened: false })

describe('groupTitle', () => {
  test('names one, two, or two plus a count', () => {
    expect(groupTitle(['A'])).toBe('A')
    expect(groupTitle(['A', 'B'])).toBe('A and B')
    expect(groupTitle(['A', 'B', 'C'])).toBe('A, B and 1 other')
    expect(groupTitle(['A', 'B', 'C', 'D'])).toBe('A, B and 2 others')
  })
})

describe('activeNow', () => {
  test('one card per busy voice channel, biggest first, members by name, who is live', () => {
    const cards = activeNow([vm(1, 10), vm(2, 11), vm(3, 11)], channels, nick, new Set([2]))
    expect(cards).toEqual([
      { channelId: 11, channel: 'gaming', userIds: [3, 2], title: 'Ana and budi', live: [2] },
      { channelId: 10, channel: 'hello', userIds: [1], title: 'Rigel', live: [] },
    ])
  })
  test('nothing when nobody is in voice', () => {
    expect(activeNow([], channels, nick, new Set())).toEqual([])
  })
})

describe('presence', () => {
  const online = new Set([1, 2, 3])
  test('streaming, in voice, online, offline', () => {
    const voice = [vm(1, 10), vm(2, 11)]
    expect(presence(1, online, voice, channels, new Set([1]))).toBe('Streaming in hello')
    expect(presence(2, online, voice, channels, new Set())).toBe('In voice · gaming')
    expect(presence(3, online, voice, channels, new Set())).toBe('Online')
    expect(presence(4, online, voice, channels, new Set())).toBe('Offline')
  })
})
