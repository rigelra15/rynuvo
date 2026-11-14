import { describe, expect, test } from 'vitest'
import { AVATAR_ICONS, avatarColor, PALETTE } from './avatar'

const keys = Object.keys(PALETTE)

describe('avatar', () => {
  test('default colour comes from the user id', () => {
    expect(avatarColor({ id: 13 })).toBe(PALETTE[keys[13 % 12] as keyof typeof PALETTE])
    expect(avatarColor({ id: 13, avatarColor: null })).toBe(avatarColor({ id: 13 }))
  })
  test('a chosen colour wins, an unknown one falls back', () => {
    expect(avatarColor({ id: 13, avatarColor: 'teal' })).toBe(PALETTE.teal)
    expect(avatarColor({ id: 13, avatarColor: 'plaid' })).toBe(avatarColor({ id: 13 }))
  })
  test('key lists match the server', () => {
    expect(keys).toEqual(['violet', 'blue', 'sky', 'teal', 'green', 'lime', 'amber', 'orange', 'red', 'pink', 'fuchsia', 'slate'])
    expect([...AVATAR_ICONS]).toEqual(['cat', 'dog', 'rabbit', 'bird', 'fish', 'turtle', 'ghost', 'skull', 'rocket', 'gamepad', 'headphones', 'music', 'coffee', 'pizza', 'flame', 'zap', 'star', 'moon', 'sun', 'leaf', 'flower', 'crown', 'anchor', 'camera'])
  })
})
