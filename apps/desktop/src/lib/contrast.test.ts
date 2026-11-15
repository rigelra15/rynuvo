import { describe, expect, test } from 'vitest'
import { PALETTE, nameColor } from './avatar'

/** WCAG contrast ratio of two #rrggbb colours. */
function contrast(a: string, b: string): number {
  const lum = (h: string) => {
    const [r, g, bl] = [1, 3, 5].map((i) => {
      const c = parseInt(h.slice(i, i + 2), 16) / 255
      return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4
    })
    return 0.2126 * r + 0.7152 * g + 0.0722 * bl
  }
  const [x, y] = [lum(a), lum(b)].sort((p, q) => q - p)
  return (x + 0.05) / (y + 0.05)
}

const ELEV = '#2a2233' // others' bubble (--elev)

describe('contrast', () => {
  test.each(Object.entries(PALETTE))('white glyph on %s avatar is at least 3.5:1', (_, hex) => {
    expect(contrast('#ffffff', hex)).toBeGreaterThanOrEqual(3.5)
  })
  test.each(Object.entries(PALETTE))('%s name on a bubble is at least 4.5:1', (_, hex) => {
    expect(contrast(nameColor(hex), ELEV)).toBeGreaterThanOrEqual(4.5)
  })
  test('text on my amber bubble is at least 7:1', () => {
    expect(contrast('#2a1a05', '#f4b860')).toBeGreaterThanOrEqual(7)
  })
  test('muted text on the sheet is at least 4.5:1', () => {
    expect(contrast('#aa9fb2', '#1a1521')).toBeGreaterThanOrEqual(4.5)
  })
  test('nameColor returns #rrggbb', () => {
    expect(nameColor('#000000')).toMatch(/^#[0-9a-f]{6}$/)
  })
})
