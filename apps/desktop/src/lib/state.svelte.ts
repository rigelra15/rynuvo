import { SvelteMap, SvelteSet } from 'svelte/reactivity'
import { api, errText, type ConnState, type SessionEvent } from './api'
import { next } from './selection'
import { dialog } from './dialog.svelte'
import type { Channel, Id, Message, User } from './types'
import { applyVoice, type MyVoice, type VoiceEvent, type VoiceMember } from './voice'

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
  /** channel id -> why its history failed to load */
  loadError = new SvelteMap<Id, string>()
  /** channel id -> live messages that arrived while its first page was loading */
  private pending = new Map<Id, Message[]>()
  typing = new SvelteMap<Id, SvelteMap<Id, number>>()
  conn = $state<ConnState>({ state: 'connecting' })
  alerts = $state<string[]>([])
  updateRequired = $state(false)
  voice = $state<VoiceMember[]>([])
  myVoice = $state<MyVoice>({ channelId: null, muted: false, deafened: false })
  speaking = new SvelteSet<Id>()
  voicePath = $state<'udp' | 'ws' | null>(null)
  micLevel = $state<{ db: number; open: boolean }>({ db: -100, open: false })
  /** user id -> volume 0–2 (1 = unchanged) */
  volumes = new SvelteMap<Id, number>()

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
    return this.users.get(id)?.nickname ?? 'someone'
  }

  async loadPrefs() {
    try {
      const p = await api.prefsGet()
      this.volumes.clear()
      for (const [id, v] of Object.entries(p.volumes)) this.volumes.set(Number(id), v)
    } catch {
      // defaults are fine
    }
  }

  async boot() {
    void this.loadPrefs()
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

  signedIn(me: User) {
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
    this.loadError.clear()
    this.pending.clear()
    this.typing.clear()
    this.alerts = []
    this.voice = []
    this.myVoice = { channelId: null, muted: false, deafened: false }
    this.speaking.clear()
    this.voicePath = null
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
    if (this.pending.has(channel)) return
    this.pending.set(channel, [])
    try {
      const list = await api.messages(channel)
      const extra = (this.pending.get(channel) ?? []).filter((m) => !list.some((x) => x.id === m.id))
      this.messages.set(channel, [...list, ...extra].sort((a, b) => a.id - b.id))
      this.hasMore.set(channel, list.length === 50)
      this.loadError.delete(channel)
    } catch (e) {
      if (!this.messages.has(channel)) this.loadError.set(channel, errText(e))
    } finally {
      this.pending.delete(channel)
    }
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
    if (!list) {
      this.pending.get(m.channelId)?.push(m) // first page in flight; merged when it lands
      return
    }
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

  inVoice(channel: Id): boolean {
    return this.myVoice.channelId === channel
  }

  private async voiceCall(f: () => Promise<MyVoice>) {
    try {
      this.myVoice = await f()
    } catch (e) {
      void dialog.notice(errText(e))
    }
  }

  joinVoice(channel: Id) {
    if (this.inVoice(channel)) return
    this.voicePath = null
    void this.voiceCall(() => api.voiceJoin(channel))
  }
  private voiceGone() {
    this.speaking.clear()
    this.micLevel = { db: -100, open: false }
    this.voicePath = null
  }

  leaveVoice() {
    this.voiceGone()
    void this.voiceCall(() => api.voiceLeave())
  }
  toggleMute() {
    void this.voiceCall(() => api.voiceSetMuted(!this.myVoice.muted))
  }
  toggleDeafen() {
    void this.voiceCall(() => api.voiceSetDeafened(!this.myVoice.deafened))
  }
  setVolume(user: Id, v: number) {
    this.volumes.set(user, v)
    void api.voiceSetVolume(user, v)
  }

  applyVoiceEvent(e: VoiceEvent) {
    switch (e.kind) {
      case 'speaking':
        this.speaking.clear()
        for (const id of e.userIds) this.speaking.add(id)
        return
      case 'level':
        this.micLevel = { db: e.db, open: e.open }
        return
      case 'path':
        this.voicePath = e.via
        return
      case 'deviceError':
        this.voiceNotice = e.message
        return
      case 'state':
        if (this.myVoice.channelId !== null && e.channelId === null) this.voiceGone()
        this.myVoice = { channelId: e.channelId, muted: e.muted, deafened: e.deafened }
        return
    }
  }

  voiceNotice = $state<string | null>(null)

  apply(e: SessionEvent) {
    switch (e.kind) {
      case 'conn':
        this.conn = e.data
        return
      case 'revoked':
        this.reset('You were removed from the circle')
        return
      case 'updateRequired':
        this.updateRequired = true
        return
      case 'server':
        break
    }
    const m = e.data
    this.voice = applyVoice(this.voice, m)
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
        // Refresh every loaded channel: edits and deletes made while offline are not replayed as events.
        for (const id of new Set([...this.messages.keys(), ...this.loadError.keys()])) void this.loadLatest(id)
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
      case 'voiceJoined':
        this.myVoice = { ...this.myVoice, channelId: m.d.channelId }
        return
      case 'voiceEnded':
        this.myVoice = { ...this.myVoice, channelId: null }
        this.voiceGone()
        this.voiceNotice = m.d.reason
        return
      case 'voiceMemberLeft':
        if (m.d.userId === this.me?.id && this.myVoice.channelId === m.d.channelId) {
          this.myVoice = { ...this.myVoice, channelId: null }
          this.voiceGone()
        }
        return
      case 'voiceMemberUpdated':
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
