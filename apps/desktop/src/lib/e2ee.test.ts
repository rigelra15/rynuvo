import { describe, expect, it } from 'vitest'
import { badgeOf } from './e2ee'

describe('badgeOf', () => {
  it('words each key status', () => {
    expect(badgeOf('verified')).toBe('Verified')
    expect(badgeOf('changed')).toBe('Key changed — verify again')
    expect(badgeOf('newDevice')).toBe('New device')
  })
  it('shows nothing for a key that is merely known', () => {
    expect(badgeOf('known')).toBeNull()
  })
})
