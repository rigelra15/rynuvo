import { describe, expect, test } from 'vitest'
import { dominantColor } from './imageColor'

const fill = (parts: [number, number, number, number][]) =>
  new Uint8ClampedArray(parts.flatMap(([r, g, b, n]) => Array.from({ length: n }, () => [r, g, b, 255]).flat()))
const hue = (hex: string) => {
  const [r, g, b] = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16))
  return { r, g, b }
}

describe('dominantColor', () => {
  test('a mostly red picture is red', () => {
    const { r, g, b } = hue(dominantColor(fill([[220, 30, 40, 80], [20, 20, 20, 20]])))
    expect(r).toBeGreaterThan(g + 60)
    expect(r).toBeGreaterThan(b + 60)
  })

  test('a little vivid colour beats a lot of dull background', () => {
    const { r, g, b } = hue(dominantColor(fill([[200, 200, 200, 70], [30, 90, 230, 30]])))
    expect(b).toBeGreaterThan(r + 60)
    expect(b).toBeGreaterThan(g)
  })

  test('black and white gives a grey', () => {
    const { r, g, b } = hue(dominantColor(fill([[0, 0, 0, 50], [255, 255, 255, 50]])))
    expect(Math.max(r, g, b) - Math.min(r, g, b)).toBeLessThan(40)
  })

  test('the shade stays readable: not near black or white', () => {
    const { r, g, b } = hue(dominantColor(fill([[60, 0, 0, 100]])))
    expect(Math.max(r, g, b)).toBeGreaterThan(120)
    const pale = hue(dominantColor(fill([[255, 200, 210, 100]])))
    expect(Math.min(pale.r, pale.g, pale.b)).toBeLessThan(160)
  })
})
