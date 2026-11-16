import { describe, expect, test } from 'vitest'
import { CHECK_EVERY_MS, checkDue, NUDGE_GAP_MS, topBar, updateStatus } from './update'

describe('update schedule', () => {
  test('notices a new version within minutes while the app stays open', () => {
    expect(CHECK_EVERY_MS).toBe(5 * 60 * 1000)
  })
  test('coming back to the app checks again, but not more than once a minute', () => {
    expect(NUDGE_GAP_MS).toBe(60 * 1000)
    expect(checkDue(null, 1_000)).toBe(true)
    expect(checkDue(1_000, 1_000 + 30_000)).toBe(false)
    expect(checkDue(1_000, 1_000 + 60_000)).toBe(true)
  })
})

describe('topBar', () => {
  const ready = { phase: 'ready', version: '0.3.1', seconds: 3 } as const
  test('outdated with a downloaded update offers the restart', () => {
    expect(topBar({ outdated: true, online: false, busy: false, update: ready })).toEqual({ kind: 'outdated', canRestart: true })
  })
  test('outdated without a download only warns', () => {
    expect(topBar({ outdated: true, online: false, busy: false, update: { phase: 'idle' } })).toEqual({ kind: 'outdated', canRestart: false })
  })
  test('offline still shows a ready update', () => {
    expect(topBar({ outdated: false, online: false, busy: false, update: ready })).toEqual({ kind: 'reconnecting', canRestart: true })
  })
  test('online and in voice: update still restarts', () => {
    expect(topBar({ outdated: false, online: true, busy: true, update: { ...ready, seconds: 3 } })).toEqual({ kind: 'update', canRestart: true })
  })
  test('nothing to say', () => {
    expect(topBar({ outdated: false, online: true, busy: false, update: { phase: 'idle' } })).toBeNull()
  })
})

describe('updateStatus (Settings)', () => {
  const idle = { phase: 'idle' } as const
  test('before asking, says nothing', () => {
    expect(updateStatus({ update: idle, checking: false, result: null, busy: false })).toEqual({ text: null, restart: false })
  })
  test('while asking', () => {
    expect(updateStatus({ update: idle, checking: true, result: null, busy: false }).text).toBe('Looking for a new version…')
  })
  test('nothing new', () => {
    expect(updateStatus({ update: idle, checking: false, result: 'latest', busy: false }).text).toBe("You're on the latest version.")
  })
  test('server unreachable', () => {
    expect(updateStatus({ update: idle, checking: false, result: 'failed', busy: false }).text).toBe("Couldn't reach the update server. Try again in a bit.")
  })
  test('downloading in the background', () => {
    expect(updateStatus({ update: { phase: 'downloading', version: '0.9.0' }, checking: false, result: 'found', busy: false }).text).toBe('Downloading 0.9.0 in the background…')
  })
  test('ready: automatically restarts regardless of voice', () => {
    const ready = { phase: 'ready', version: '0.9.0', seconds: 3 } as const
    expect(updateStatus({ update: ready, checking: false, result: null, busy: false })).toEqual({ text: '0.9.0 is ready. Restarting in 3s…', restart: false })
    expect(updateStatus({ update: ready, checking: false, result: null, busy: true })).toEqual({ text: '0.9.0 is ready. Restarting in 3s…', restart: false })
  })
})
