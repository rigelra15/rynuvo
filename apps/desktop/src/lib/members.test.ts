import { describe, expect, test } from 'vitest'
import { memberGroups } from './members'

const u = (id: number, nickname: string) => ({ id, nickname, isAdmin: false })

describe('memberGroups', () => {
  test('splits online and offline, each sorted by name ignoring case', () => {
    const users = [u(1, 'zara'), u(2, 'Budi'), u(3, 'andi'), u(4, 'Citra')]
    const g = memberGroups(users, new Set([1, 3]))
    expect(g.online.map((x) => x.nickname)).toEqual(['andi', 'zara'])
    expect(g.offline.map((x) => x.nickname)).toEqual(['Budi', 'Citra'])
  })

  test('I always count as online (presence for myself can lag on connect)', () => {
    const g = memberGroups([u(1, 'me'), u(2, 'b')], new Set(), 1)
    expect(g.online.map((x) => x.id)).toEqual([1])
    expect(g.offline.map((x) => x.id)).toEqual([2])
  })
})
