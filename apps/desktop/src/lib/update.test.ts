import { describe, expect, test } from 'vitest'
import { CHECK_EVERY_MS, topBar } from './update'

describe('update schedule', () => {
  test('checks a few times a day', () => {
    expect(CHECK_EVERY_MS).toBe(6 * 60 * 60 * 1000)
  })
})

describe('topBar', () => {
  const ready = { phase: 'ready', version: '0.3.1' } as const
  test('outdated with a downloaded update offers the restart', () => {
    expect(topBar({ outdated: true, online: false, busy: false, update: ready })).toEqual({ kind: 'outdated', canRestart: true })
  })
  test('outdated without a download only warns', () => {
    expect(topBar({ outdated: true, online: false, busy: false, update: { phase: 'idle' } })).toEqual({ kind: 'outdated', canRestart: false })
  })
  test('offline still shows a ready update', () => {
    expect(topBar({ outdated: false, online: false, busy: false, update: ready })).toEqual({ kind: 'reconnecting', canRestart: true })
  })
  test('online and in voice: update shown, no restart', () => {
    expect(topBar({ outdated: false, online: true, busy: true, update: ready })).toEqual({ kind: 'update', canRestart: false })
  })
  test('nothing to say', () => {
    expect(topBar({ outdated: false, online: true, busy: false, update: { phase: 'idle' } })).toBeNull()
  })
})
