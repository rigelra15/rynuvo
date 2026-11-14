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
