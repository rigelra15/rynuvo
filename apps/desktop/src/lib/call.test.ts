import { describe, expect, it } from 'vitest'
import { callStart } from './call'

const m = (userId: number, joinedAt?: number) => ({ userId, channelId: 5, muted: false, deafened: false, joinedAt })

describe('callStart', () => {
  it('is when the longest-staying member joined', () => {
    expect(callStart([m(1, 5000), m(2, 2000), m(3, 9000)])).toBe(2000)
  })
  it("is when the call began, even after the one who started it left", () => {
    const a = { ...m(2, 5000), callSince: 1000 }
    const b = { ...m(3, 9000), callSince: 1000 }
    expect(callStart([a, b])).toBe(1000)
  })
  it('is unknown with nobody, or from an older server that sends no join times', () => {
    expect(callStart([])).toBeNull()
    expect(callStart([m(1), m(2, 0)])).toBeNull()
  })
})
