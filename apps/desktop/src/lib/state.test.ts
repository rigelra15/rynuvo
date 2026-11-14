import { beforeEach, describe, expect, test, vi } from 'vitest'
import type { Message } from './types'

const messages = vi.fn()
vi.mock('./api', () => ({
  api: { messages: (...a: unknown[]) => messages(...a) },
  errText: (e: unknown) => String(e),
}))

const { store } = await import('./state.svelte')

const msg = (id: number, channelId = 1): Message => ({ id, channelId, authorId: 2, content: `m${id}`, image: null, createdAt: 0, editedAt: null })
const ready = {
  kind: 'server' as const,
  data: {
    t: 'ready' as const,
    d: { me: { id: 1, nickname: 'me', isAdmin: false }, users: [], online: [], alerts: [], channels: [{ id: 1, kind: 'text' as const, name: 'a', topic: null, ownerId: 1, position: 1 }] },
  },
}
const flush = () => new Promise((r) => setTimeout(r, 0))

describe('store', () => {
  beforeEach(() => {
    store.reset(null)
    messages.mockReset()
  })

  test('ready reloads cached channels so offline edits and deletes show', async () => {
    messages.mockResolvedValueOnce([msg(1), msg(2)])
    store.apply(ready)
    await flush()
    expect(store.messages.get(1)?.map((m) => m.id)).toEqual([1, 2])
    messages.mockResolvedValueOnce([msg(1)]) // message 2 was deleted while offline
    store.apply(ready)
    await flush()
    expect(store.messages.get(1)?.map((m) => m.id)).toEqual([1])
  })

  test('failed load shows an error and retries on ready', async () => {
    messages.mockRejectedValueOnce('offline')
    store.apply(ready)
    await flush()
    expect(store.loadError.get(1)).toBe('offline')
    messages.mockResolvedValueOnce([msg(1)])
    store.apply(ready)
    await flush()
    expect(store.loadError.has(1)).toBe(false)
    expect(store.messages.get(1)?.map((m) => m.id)).toEqual([1])
  })

  test('messages arriving during the first load are kept', async () => {
    let resolve!: (v: Message[]) => void
    messages.mockReturnValueOnce(new Promise((r) => (resolve = r)))
    store.apply(ready)
    store.apply({ kind: 'server', data: { t: 'messageCreated', d: msg(3) } })
    resolve([msg(1), msg(2)])
    await flush()
    expect(store.messages.get(1)?.map((m) => m.id)).toEqual([1, 2, 3])
  })
})

describe('voice ownership', () => {
  beforeEach(() => store.reset(null))

  test('server voice messages do not move my channel; only engine state events do', () => {
    store.apply(ready)
    store.apply({ kind: 'server', data: { t: 'voiceJoined', d: { channelId: 5, session: 1, key: 'k', udpPort: 3478 } } } as never)
    expect(store.myVoice.channelId).toBe(null)
    store.applyVoiceEvent({ kind: 'state', channelId: 5, muted: false, deafened: false })
    expect(store.myVoice.channelId).toBe(5)
    store.apply({ kind: 'server', data: { t: 'voiceMemberLeft', d: { userId: 1, channelId: 5 } } } as never)
    expect(store.myVoice.channelId).toBe(5)
  })

  test('ready_after_channels_and_first_page', async () => {
    store.phase = 'boot'
    expect(store.ready).toBe(false)
    store.phase = 'onboarding'
    expect(store.ready).toBe(true)
    store.phase = 'app'
    expect(store.ready).toBe(false) // no channel list yet
    let resolve!: (v: Message[]) => void
    messages.mockReturnValueOnce(new Promise((r) => (resolve = r)))
    store.apply(ready)
    expect(store.ready).toBe(false) // channels in, selected channel's messages not yet
    resolve([msg(1)])
    await flush()
    expect(store.ready).toBe(true)
  })

  test('ready when the first page fails, so the error shows instead of the splash', async () => {
    store.phase = 'app'
    messages.mockRejectedValueOnce('offline')
    store.apply(ready)
    await flush()
    expect(store.ready).toBe(true)
  })

  test('ready after the boot timeout even with no connection', () => {
    store.phase = 'app'
    expect(store.ready).toBe(false)
    store.bootTimedOut = true
    expect(store.ready).toBe(true)
  })
})
