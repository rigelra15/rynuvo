import { SvelteMap, SvelteSet } from 'svelte/reactivity'
import { api, errText, type ConnState, type SessionEvent } from './api'
import { next } from './selection'
import { dialog } from './dialog.svelte'
import type { Channel, Id, Message, User } from './types'
import { applyVoice, type MyVoice, type VoiceEvent, type VoiceMember } from './voice'
import { applyStreams, type StreamEvent, type StreamInfo } from './stream'
import type { MusicCmd, MusicState, MusicStatus } from './music'
import { videos } from './videos'

export const TYPING_TTL_MS = 6000

class Store {
  phase = $state<'boot' | 'onboarding' | 'app'>('boot')
  notice = $state<string | null>(null)
  server = $state('')
  me = $state<User | null>(null)
  users = new SvelteMap<Id, User>()
  online = new SvelteSet<Id>()
  channels = $state<Channel[]>([])
  /** The first `ready` of this sign-in has arrived (the channel list is real). */
  gotReady = $state(false)
  /** Boot took too long (server down): stop waiting and show the app with its reconnect bar. */
  bootTimedOut = $state(false)
  selectedId = $state<Id | null>(null)
  /** Which page the rail shows: the circle's channels, or Home (friends and activity). */
  view = $state<'circle' | 'home'>('circle')
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
  /** Live streams (Go Live) in every voice channel. */
  streams = $state<StreamInfo[]>([])
  /** My own stream while I'm live. */
  live = $state<{ viewers: Id[]; codec: string; kbps: number; hardware: boolean; relay: boolean } | null>(null)
  goingLive = $state(false)
  /** The streamer whose stream is open in the stream view. */
  watching = $state<Id | null>(null)
  streamStats = new SvelteMap<Id, { fps: number; codec: string; kbps: number; path: string }>()
  streamNotice = $state<string | null>(null)
  streamPreset = $state('1080p60')
  /** Listen-together music per voice channel, with the local time each state arrived. */
  music = new SvelteMap<Id, { state: MusicState; receivedAt: number }>()
  /** The "Rynuvo Music" user: authors the bot's replies, never listed as a person. */
  musicBot = $state<Id | null>(null)
  musicStatus = new SvelteMap<string, MusicStatus>()
  musicVolume = $state(0.5)
  /** Show the queue panel in the music view. */
  queueOpen = $state(false)

  /** The splash can go: onboarding is up, or the channel list and the open text channel's first page are in. */
  get ready(): boolean {
    if (this.phase === 'onboarding') return true
    if (this.phase !== 'app') return false
    if (this.bootTimedOut) return true
    if (!this.gotReady) return false
    const c = this.selected
    return !c || c.kind !== 'text' || this.messages.has(c.id) || this.loadError.has(c.id)
  }

  get selected(): Channel | null {
    return this.channels.find((c) => c.id === this.selectedId) ?? null
  }

  sortedChannels(): Channel[] {
    return [...this.channels].sort(
      (a, b) => (a.kind === b.kind ? 0 : a.kind === 'text' ? -1 : 1) || a.position - b.position || a.id - b.id,
    )
  }

  canManage = (c: Channel): boolean => !!this.me && (this.me.isAdmin || c.ownerId === this.me.id)

  /** Everyone in the circle, without the music bot. */
  people(): User[] {
    return [...this.users.values()].filter((u) => u.id !== this.musicBot)
  }

  musicIn(channel: Id | null): { state: MusicState; receivedAt: number } | null {
    if (channel === null) return null
    const m = this.music.get(channel)
    return m && m.state.now ? m : null
  }

  setMusicVolume(v: number) {
    this.musicVolume = v
    void api.musicSetVolume(v)
  }

  /** A music control from the player (replies go to the voice channel's last command channel). */
  musicCmd(cmd: MusicCmd) {
    void api.musicCmd(cmd, null).catch((e) => dialog.notice(errText(e)))
  }

  user(id: Id): User | undefined {
    return this.users.get(id)
  }

  /** Save my avatar choice; everyone else hears it through `userUpserted`. */
  async setAvatar(p: { avatarIcon?: string | null; avatarColor?: string | null }) {
    const u = await api.setProfile(p)
    this.me = u
    this.users.set(u.id, u)
  }

  nick(id: Id): string {
    return this.users.get(id)?.nickname ?? 'someone'
  }

  async loadPrefs() {
    try {
      const p = await api.prefsGet()
      this.volumes.clear()
      for (const [id, v] of Object.entries(p.volumes)) this.volumes.set(Number(id), v)
      if (p.streamPreset) this.streamPreset = p.streamPreset
      if (typeof p.musicVolume === 'number') this.musicVolume = p.musicVolume
    } catch {
      // defaults are fine
    }
  }

  async boot() {
    void this.loadPrefs()
    setTimeout(() => (this.bootTimedOut = true), 8000)
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
    this.gotReady = false
    this.selectedId = null
    this.view = 'circle'
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
    this.streams = []
    this.live = null
    this.watching = null
    this.streamStats.clear()
    this.music.clear()
    this.musicBot = null
    this.musicStatus.clear()
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
  isLive(user: Id): boolean {
    return this.streams.some((s) => s.userId === user)
  }

  async goLive(sourceId: string, preset: string): Promise<boolean> {
    this.goingLive = true
    try {
      await api.streamGoLive(sourceId, preset)
      this.live = { viewers: [], codec: '', kbps: 0, hardware: false, relay: false }
      return true
    } catch (e) {
      void dialog.notice(errText(e))
      return false
    } finally {
      this.goingLive = false
    }
  }

  stopLive() {
    this.live = null
    void api.streamStop()
  }

  watch(user: Id) {
    if (this.me && user === this.me.id) return
    this.streamNotice = null
    this.watching = user
  }

  closeStream() {
    this.watching = null
  }

  applyStreamEvent(e: StreamEvent) {
    switch (e.kind) {
      case 'live':
        if (this.live) this.live = { viewers: e.viewers, codec: e.codec, kbps: e.kbps, hardware: e.hardware, relay: e.relay }
        return
      case 'stats':
        this.streamStats.set(e.userId, { fps: e.fps, codec: e.codec, kbps: e.kbps, path: e.path })
        return
      case 'ended':
        this.streamStats.delete(e.userId)
        if (this.me && e.userId === this.me.id) {
          this.live = null
        } else if (this.watching === e.userId) {
          this.watching = null
          this.streamNotice = `${this.nick(e.userId)}'s stream ended`
        }
        return
      case 'error':
        this.live = null
        this.streamNotice = e.message
        return
    }
  }

  private voiceGone() {
    this.live = null
    this.watching = null
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
    this.streams = applyStreams(this.streams, m)
    switch (m.t) {
      case 'ready': {
        this.me = m.d.me
        this.users.clear()
        for (const u of m.d.users) this.users.set(u.id, u)
        this.online.clear()
        for (const id of m.d.online) this.online.add(id)
        this.channels = m.d.channels
        this.gotReady = true
        this.alerts = m.d.alerts
        this.musicBot = m.d.musicBot ?? null
        this.music.clear()
        for (const s of m.d.music ?? []) {
          this.music.set(s.channelId, { state: s, receivedAt: Date.now() })
          videos.prefetch(s)
        }
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
      // My own channel comes only from the engine's `state` events (VoiceCtl owns it).
      case 'voiceJoined':
        return
      case 'voiceEnded':
        this.voiceNotice = m.d.reason
        return
      case 'voiceMemberLeft':
        return
      case 'voiceMemberUpdated':
        return
      case 'streamStarted':
      case 'streamStopped':
      case 'streamViewer':
      case 'streamSignal':
        return
      case 'musicState':
        if (!m.d.now && !m.d.queue.length) this.music.delete(m.d.channelId)
        else {
          this.music.set(m.d.channelId, { state: m.d, receivedAt: Date.now() })
          videos.prefetch(m.d)
        }
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
