import { beforeEach, describe, expect, test, vi } from 'vitest'
import type { Message } from './types'

const messages = vi.fn()
const markRead = vi.fn((..._a: unknown[]) => Promise.resolve())
vi.mock('./api', () => ({
  api: { messages: (...a: unknown[]) => messages(...a), markRead: (...a: unknown[]) => markRead(...a) },
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

  test('voice_notice_shows_the_server_update_message', () => {
    store.applyVoiceEvent({ kind: 'notice', message: 'The server needs an update for encrypted calls' })
    expect(store.voiceNotice).toBe('The server needs an update for encrypted calls')
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

  test('with no connection the splash stays up, however long it takes', () => {
    store.phase = 'app'
    expect(store.ready).toBe(false)
    store.bootSlow = true
    expect(store.ready).toBe(false)
  })

  test('a call I am in from another device counts as elsewhere, not joined', () => {
    store.me = { id: 1, nickname: 'me', isAdmin: false }
    store.voice = [{ userId: 1, channelId: 3, muted: false, deafened: false }]
    store.myVoice = { channelId: null, muted: false, deafened: false }
    expect(store.inVoiceElsewhere(3)).toBe(true)
    expect(store.inVoiceElsewhere(4)).toBe(false)
    store.myVoice = { channelId: 3, muted: false, deafened: false }
    expect(store.inVoiceElsewhere(3)).toBe(false)
  })})

describe('unread', () => {
  const two = (reads: { channelId: number; lastMessage: number | null; lastRead: number }[]) => ({
    kind: 'server' as const,
    data: {
      t: 'ready' as const,
      d: {
        ...ready.data.d,
        users: [{ id: 1, nickname: 'me', isAdmin: false }, { id: 2, nickname: 'budi', isAdmin: false }],
        channels: [ready.data.d.channels[0], { id: 5, kind: 'text' as const, name: 'b', topic: null, ownerId: 1, position: 2 }],
        reads,
      },
    },
  })
  const created = (m: Message) => store.apply({ kind: 'server', data: { t: 'messageCreated', d: m } })

  beforeEach(() => {
    store.reset(null)
    store.phase = 'app'
    store.looking = true
    messages.mockReset()
    messages.mockResolvedValue([])
    markRead.mockClear()
  })

  test('a channel with messages past my read mark is unread; the open one gets read', async () => {
    store.apply(two([{ channelId: 1, lastMessage: 4, lastRead: 4 }, { channelId: 5, lastMessage: 9, lastRead: 7 }]))
    await flush()
    store.seeSelected() // channel 1 is open and loaded
    expect(store.isUnread(1)).toBe(false)
    expect(store.isUnread(5)).toBe(true)
    store.select(5)
    await flush()
    store.seeSelected()
    expect(store.isUnread(5)).toBe(false)
    expect(markRead).toHaveBeenCalledWith(5, 9)
  })

  test('nothing is read while I am not looking', async () => {
    store.apply(two([{ channelId: 1, lastMessage: 4, lastRead: 4 }, { channelId: 5, lastMessage: null, lastRead: 0 }]))
    await flush()
    store.looking = false
    created(msg(10, 1))
    store.seeSelected()
    expect(store.isUnread(1)).toBe(true)
    expect(markRead).not.toHaveBeenCalled()
  })

  test('my own message never makes a channel unread', async () => {
    store.apply(two([{ channelId: 1, lastMessage: 4, lastRead: 4 }, { channelId: 5, lastMessage: 3, lastRead: 3 }]))
    await flush()
    created({ ...msg(11, 5), authorId: 1 })
    expect(store.isUnread(5)).toBe(false)
  })

  test('a mention of me or @everyone marks the channel, until it is read anywhere', async () => {
    store.apply(two([{ channelId: 1, lastMessage: 4, lastRead: 4 }, { channelId: 5, lastMessage: 3, lastRead: 3 }]))
    await flush()
    created({ ...msg(12, 5), content: 'yo @budi' })
    expect(store.isMentioned(5)).toBe(false)
    created({ ...msg(13, 5), content: 'hey @me' })
    expect(store.isMentioned(5)).toBe(true)
    store.apply({ kind: 'server', data: { t: 'readUpdated', d: { channelId: 5, lastRead: 13 } } })
    expect(store.isUnread(5)).toBe(false)
    expect(store.isMentioned(5)).toBe(false)
    created({ ...msg(14, 5), content: '@everyone rapat' })
    expect(store.isMentioned(5)).toBe(true)
  })

  test('at connect, mentions among the unread are found', async () => {
    messages.mockImplementation(async (ch: number) => (ch === 5 ? [{ ...msg(6, 5), content: '@me cek' }, msg(8, 5)] : []))
    store.apply(two([{ channelId: 1, lastMessage: 4, lastRead: 4 }, { channelId: 5, lastMessage: 8, lastRead: 5 }]))
    await flush()
    expect(store.isMentioned(5)).toBe(true)
  })
})
