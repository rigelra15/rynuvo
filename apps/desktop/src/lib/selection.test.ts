import { describe, expect, test } from 'vitest'
import { next } from './selection'
import type { Channel } from './types'

const ch = (id: number, kind: 'text' | 'voice', position: number): Channel => ({ id, kind, name: `c${id}`, topic: null, ownerId: 1, position })

describe('selection', () => {
  test('selection falls back when channel deleted', () => {
    const list = [ch(1, 'text', 1), ch(2, 'text', 2), ch(3, 'text', 3), ch(9, 'voice', 1)]
    expect(next(list, 2, 2)).toBe(1)
    expect(next(list, 1, 1)).toBe(2)
    expect(next(list, 3, 1)).toBe(3) // deleting another channel keeps selection
    expect(next([ch(5, 'text', 1), ch(9, 'voice', 1)], 5, 5)).toBeNull()
  })
})
