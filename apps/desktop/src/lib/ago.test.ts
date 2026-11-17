import { expect, test } from 'vitest'
import { ago } from './ago'

const M = 60_000
test('short last-seen chips', () => {
  expect(ago(0, 10_000)).toBe('1m')
  expect(ago(0, 3 * M)).toBe('3m')
  expect(ago(0, 59 * M)).toBe('59m')
  expect(ago(0, 60 * M)).toBe('1h')
  expect(ago(0, 23 * 60 * M + 59 * M)).toBe('23h')
  expect(ago(0, 24 * 60 * M)).toBe('1d')
  expect(ago(0, 8 * 24 * 60 * M)).toBe('1w')
  expect(ago(0, 45 * 24 * 60 * M)).toBe('1mo')
  expect(ago(0, 400 * 24 * 60 * M)).toBe('1y')
})
