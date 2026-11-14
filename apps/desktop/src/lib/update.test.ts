import { describe, expect, test } from 'vitest'
import { banner, CHECK_EVERY_MS } from './update'

describe('update banner', () => {
  test('nothing to show while idle or checking', () => {
    expect(banner({ phase: 'idle' }, false)).toBeNull()
    expect(banner({ phase: 'downloading', version: '0.3.1' }, false)).toBeNull()
  })

  test('ready offers a restart outside voice', () => {
    expect(banner({ phase: 'ready', version: '0.3.1' }, false)).toEqual({
      text: 'Update ready (0.3.1)',
      canRestart: true,
      hint: '',
    })
  })

  test('never restarts while in voice or live', () => {
    const b = banner({ phase: 'ready', version: '0.3.1' }, true)
    expect(b?.canRestart).toBe(false)
    expect(b?.hint).toBe('Leave voice to restart')
  })

  test('checks a few times a day', () => {
    expect(CHECK_EVERY_MS).toBe(6 * 60 * 60 * 1000)
  })
})
