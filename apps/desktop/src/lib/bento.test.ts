import { describe, expect, test } from 'vitest'
import { seats } from './bento'

describe('seats', () => {
  test('a few people sit in one row, as big as the room allows but not huge', () => {
    const s = seats(3, 900, 600, 24)
    expect(s.cols).toBe(3)
    expect(s.size).toBeLessThanOrEqual(168)
    expect(s.size).toBeGreaterThanOrEqual(120)
  })
  test('six people make two rows of three', () => {
    expect(seats(6, 900, 600, 24).cols).toBe(3)
  })
  test('seats shrink to fit a small room and never below 48px', () => {
    const s = seats(4, 300, 200, 24)
    expect(s.size * s.cols + 24 * (s.cols - 1)).toBeLessThanOrEqual(300)
    expect(seats(12, 120, 90, 24).size).toBe(48)
  })
  test('nobody: no seats', () => {
    expect(seats(0, 900, 600, 24).size).toBe(0)
  })
})
