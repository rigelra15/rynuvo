import { describe, expect, test } from 'vitest'
import { byDay, dayLabel, isHead, newDay } from './chat'

const at = (y: number, mo: number, d: number, h = 12, mi = 0) => new Date(y, mo - 1, d, h, mi).getTime()
const msg = (authorId: number, createdAt: number) => ({ authorId, createdAt })

describe('chat grouping', () => {
  test('a run breaks on a new author or a 5 minute gap', () => {
    const t = at(2026, 11, 14, 10)
    expect(isHead(undefined, msg(1, t))).toBe(true)
    expect(isHead(msg(1, t), msg(2, t + 1000))).toBe(true)
    expect(isHead(msg(1, t), msg(1, t + 4 * 60_000))).toBe(false)
    expect(isHead(msg(1, t), msg(1, t + 6 * 60_000))).toBe(true)
  })

  test('a new day starts at local midnight', () => {
    expect(newDay(undefined, msg(1, at(2026, 11, 14)))).toBe(true)
    expect(newDay(msg(1, at(2026, 11, 13, 23, 59)), msg(1, at(2026, 11, 14, 0, 1)))).toBe(true)
    expect(newDay(msg(1, at(2026, 11, 14, 1)), msg(1, at(2026, 11, 14, 23)))).toBe(false)
  })

  test('day labels', () => {
    const now = at(2026, 11, 14, 15)
    expect(dayLabel(at(2026, 11, 14, 9), now)).toBe('Today')
    expect(dayLabel(at(2026, 11, 13, 22), now)).toBe('Yesterday')
    expect(dayLabel(at(2026, 11, 12), now)).toBe('12 Nov')
    expect(dayLabel(at(2025, 11, 12), now)).toBe('12 Nov 2025')
  })
})

describe('byDay', () => {
  test('splits messages into local days and marks run heads, a new day always starting a run', () => {
    const list = [msg(1, at(2026, 11, 13, 23, 58)), msg(1, at(2026, 11, 13, 23, 59)), msg(1, at(2026, 11, 14, 0, 1)), msg(2, at(2026, 11, 14, 0, 2))]
    const days = byDay(list)
    expect(days.map((d) => d.items.length)).toEqual([2, 2])
    expect(days.map((d) => d.at)).toEqual([list[0].createdAt, list[2].createdAt])
    expect(days.flatMap((d) => d.items.map((i) => i.head))).toEqual([true, false, true, true])
    expect(days[1].items[0].m).toBe(list[2])
  })

  test('empty list has no days', () => {
    expect(byDay([])).toEqual([])
  })
})
