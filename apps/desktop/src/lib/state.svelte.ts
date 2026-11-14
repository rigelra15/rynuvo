import { SvelteMap, SvelteSet } from 'svelte/reactivity'
import { api, errText, type ConnState, type SessionEvent } from './api'
import { next } from './selection'
import type { Channel, Id, Message, User } from './types'

export const TYPING_TTL_MS = 6000

class Store {
  phase = $state<'boot' | 'onboarding' | 'app'>('boot')
  notice = $state<string | null>(null)
  server = $state('')
  me = $state<User | null>(null)
  users = new SvelteMap<Id, User>()
  online = new SvelteSet<Id>()
  channels = $state<Channel[]>([])
  selectedId = $state<Id | null>(null)
  messages = new SvelteMap<Id, Message[]>()
  hasMore = new SvelteMap<Id, boolean>()
  typing = new SvelteMap<Id, SvelteMap<Id, number>>()
  conn = $state<ConnState>({ state: 'connecting' })
  alerts = $state<string[]>([])
  updateRequired = $state(false)

  get selected(): Channel | null {
    return this.channels.find((c) => c.id === this.selectedId) ?? null
  }

  sortedChannels(): Channel[] {
    return [...this.channels].sort(
      (a, b) => (a.kind === b.kind ? 0 : a.kind === 'text' ? -1 : 1) || a.position - b.position || a.id - b.id,
    )
  }

  canManage = (c: Channel): boolean => !!this.me && (this.me.isAdmin || c.ownerId === this.me.id)

  nick(id: Id): string {
    return this.users.get(id)?.nickname ?? 'seseorang'
  }

  async boot() {
    try {
      const info = await api.bootstrap()
      if (!info) {
        this.phase = 'onboarding'
        return
      }
      this.server = info.server
      if (info.me) this.me = info.me
      this.phase = 'app'
    } catch (e) {
      this.notice = errText(e)
      this.phase = 'onboarding'
    }
  }

  signedIn(server: string, me: User) {
    this.server = server
    this.me = me
    this.notice = null
    this.phase = 'app'
  }

  reset(notice: string | null) {
    this.me = null
    this.users.clear()
    this.online.clear()
    this.channels = []
    this.selectedId = null
    this.messages.clear()
    this.hasMore.clear()
    this.typing.clear()
    this.alerts = []
    this.notice = notice
    this.phase = 'onboarding'
  }

  async logout() {
    await api.logout()
    this.reset(null)
  }

  select(id: Id | null) {
    this.selectedId = id
    const c = this.selected
    if (c && c.kind === 'text' && !this.messages.has(c.id)) void this.loadLatest(c.id)
  }

  async loadLatest(channel: Id) {
    const list = await api.messages(channel)
    this.messages.set(channel, list)
    this.hasMore.set(channel, list.length === 50)
  }

  async loadOlder(channel: Id): Promise<number> {
    const cur = this.messages.get(channel) ?? []
    if (!cur.length || this.hasMore.get(channel) === false) return 0
    const older = await api.messages(channel, cur[0].id)
    this.messages.set(channel, [...older, ...cur])
    this.hasMore.set(channel, older.length === 50)
    return older.length
  }

  private upsertMessage(m: Message) {
    const list = this.messages.get(m.channelId)
    if (!list) return // not loaded; will fetch on open
    const i = list.findIndex((x) => x.id === m.id)
    if (i >= 0) {
      const copy = list.slice()
      copy[i] = m
      this.messages.set(m.channelId, copy)
    } else {
      this.messages.set(m.channelId, [...list, m].sort((a, b) => a.id - b.id))
    }
  }

  private removeChannel(id: Id) {
    const newSel = next(this.channels, this.selectedId, id)
    this.channels = this.channels.filter((c) => c.id !== id)
    this.messages.delete(id)
    this.typing.delete(id)
    if (this.selectedId === id || this.selectedId === null) this.select(newSel)
  }

  apply(e: SessionEvent) {
    switch (e.kind) {
      case 'conn':
        this.conn = e.data
        return
      case 'revoked':
        this.reset('Kamu dikeluarkan dari circle')
        return
      case 'updateRequired':
        this.updateRequired = true
        return
      case 'server':
        break
    }
    const m = e.data
    switch (m.t) {
      case 'ready': {
        this.me = m.d.me
        this.users.clear()
        for (const u of m.d.users) this.users.set(u.id, u)
        this.online.clear()
        for (const id of m.d.online) this.online.add(id)
        this.channels = m.d.channels
        this.alerts = m.d.alerts
        // drop caches of channels that disappeared while offline
        for (const id of [...this.messages.keys()]) if (!m.d.channels.some((c) => c.id === id)) this.messages.delete(id)
        if (!this.selected) this.select(next(this.channels, null, -1))
        return
      }
      case 'messageCreated':
      case 'messageUpdated':
        this.upsertMessage(m.d)
        this.typing.get(m.d.channelId)?.delete(m.d.authorId)
        return
      case 'messageDeleted': {
        const list = this.messages.get(m.d.channelId)
        if (list) this.messages.set(m.d.channelId, list.filter((x) => x.id !== m.d.id))
        return
      }
      case 'channelCreated':
        if (!this.channels.some((c) => c.id === m.d.id)) this.channels = [...this.channels, m.d]
        return
      case 'channelUpdated':
        this.channels = this.channels.map((c) => (c.id === m.d.id ? m.d : c))
        return
      case 'channelDeleted':
        this.removeChannel(m.d.id)
        return
      case 'userUpserted':
        this.users.set(m.d.id, m.d)
        if (this.me?.id === m.d.id) this.me = m.d
        return
      case 'userRemoved':
        this.users.delete(m.d.id)
        this.online.delete(m.d.id)
        return
      case 'presence':
        if (m.d.online) this.online.add(m.d.userId)
        else this.online.delete(m.d.userId)
        return
      case 'typing': {
        let t = this.typing.get(m.d.channelId)
        if (!t) {
          t = new SvelteMap()
          this.typing.set(m.d.channelId, t)
        }
        t.set(m.d.userId, Date.now() + TYPING_TTL_MS)
        return
      }
      case 'adminAlert':
        this.alerts = [...this.alerts, m.d.message]
        return
      case 'pong':
        return
    }
  }

  typingNames(channel: Id, now: number): string[] {
    const t = this.typing.get(channel)
    if (!t) return []
    return [...t.entries()].filter(([id, exp]) => exp > now && id !== this.me?.id).map(([id]) => this.nick(id))
  }
}

export const store = new Store()
