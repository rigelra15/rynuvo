import { describe, expect, it, test } from 'vitest'
import { bento, fill } from './bento'

describe('bento', () => {
  test('one person fills the stage at 16:9', () => {
    expect(bento(1, 1600, 900, 8)).toEqual({ cols: 1, rows: 1, w: 1600, h: 900 })
    // too wide: height limits
    expect(bento(1, 2000, 900, 8)).toEqual({ cols: 1, rows: 1, w: 1600, h: 900 })
  })
  test('picks the column count that gives the biggest tiles', () => {
    expect(bento(2, 1200, 600, 0)).toMatchObject({ cols: 2, rows: 1 }) // side by side
    expect(bento(2, 600, 1000, 0)).toMatchObject({ cols: 1, rows: 2 }) // stacked on a tall stage
    expect(bento(4, 1600, 900, 0)).toMatchObject({ cols: 2, rows: 2, w: 800, h: 450 })
    expect(bento(8, 1600, 900, 0)).toMatchObject({ cols: 3, rows: 3 })
  })
  test('tiles always fit inside the stage, gaps included', () => {
    for (const n of [1, 2, 3, 5, 7, 8, 12]) {
      const g = bento(n, 900, 500, 12)
      expect(g.cols * g.w + (g.cols - 1) * 12).toBeLessThanOrEqual(900 + 0.5)
      expect(g.rows * g.h + (g.rows - 1) * 12).toBeLessThanOrEqual(500 + 0.5)
      expect(g.cols * g.rows).toBeGreaterThanOrEqual(n)
    }
  })
  test('empty or unmeasured stage gives no size', () => {
    expect(bento(0, 900, 500, 8)).toEqual({ cols: 1, rows: 0, w: 0, h: 0 })
    expect(bento(3, 0, 0, 8).w).toBe(0)
  })
})

describe('fill', () => {
  it('stretches the chosen columns and rows over the whole area', () => {
    const b = bento(1, 1000, 700, 10)
    expect(fill(b, 1000, 700, 10)).toEqual({ ...b, w: 1000, h: 700 })
    const four = bento(4, 1000, 700, 10)
    const f = fill(four, 1000, 700, 10)
    expect(f.cols * f.w + (f.cols - 1) * 10).toBeLessThanOrEqual(1000)
    expect(f.rows * f.h + (f.rows - 1) * 10).toBeLessThanOrEqual(700)
    expect(f.w).toBeGreaterThan(four.w - 1)
  })
})
