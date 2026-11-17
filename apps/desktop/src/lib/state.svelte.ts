import { SvelteMap, SvelteSet } from 'svelte/reactivity'
import { api, errText, type ConnState, type SessionEvent } from './api'
import { next } from './selection'
import { dialog } from './dialog.svelte'
import type { CallSecurity, Channel, Id, Message, ProfilePatch, User } from './types'
import { applyVoice, type MyVoice, type VoiceEvent, type VoiceMember } from './voice'
import { applyStreams, type StreamEvent, type StreamInfo } from './stream'
import type { MusicCmd, MusicState, MusicStatus } from './music'
import { videos } from './videos'
import { mentionsMe } from './mention'

export const TYPING_TTL_MS = 6000

class Store {
  phase = $state<'boot' | 'onboarding' | 'app'>('boot')
  notice = $state<string | null>(null)
  server = $state('')
  me = $state<User | null>(null)
  users = new SvelteMap<Id, User>()
  online = new SvelteSet<Id>()
  /** Of `online`: those on only a phone or tablet. */
  mobile = new SvelteSet<Id>()
  /** When each user was last connected (server ms). */
  lastSeen = new SvelteMap<Id, number>()
  channels = $state<Channel[]>([])
  /** The first `ready` of this sign-in has arrived (the channel list is real). */
  gotReady = $state(false)
  /** Boot is taking a while (no internet, or the server is down): the splash says so and keeps waiting. */
  bootSlow = $state(false)
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
  /** Text channel -> how far I've read it (from the server, so the same on all my devices). */
  reads = new SvelteMap<Id, { last: Id | null; read: Id }>()
  /** Text channels whose unread messages mention me or @everyone. */
  mentioned = new SvelteSet<Id>()
  /** The open channel is really on screen (App: window shown, and on a phone the chat pane is open). */
  looking = $state(true)
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
  streamCursor = $state(true)
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
    if (!this.gotReady) return false
    const c = this.selected
    return !c || c.kind !== 'text' || this.messages.has(c.id) || this.loadError.has(c.id)
  }

  /** A text channel has messages I haven't seen. */
  isUnread(channel: Id): boolean {
    const r = this.reads.get(channel)
    return !!r && r.last !== null && r.last > r.read
  }

  /** …and one of them mentions me. */
  isMentioned(channel: Id): boolean {
    return this.mentioned.has(channel) && this.isUnread(channel)
  }

  /** I'm looking at the open text channel: it's read up to its newest message. */
  seeSelected() {
    const c = this.selected
    if (!c || c.kind !== 'text' || this.view !== 'circle' || !this.looking || !this.messages.has(c.id)) return
    const r = this.reads.get(c.id)
    this.mentioned.delete(c.id)
    if (!r || r.last === null || r.last <= r.read) return
    this.reads.set(c.id, { ...r, read: r.last })
    void api.markRead(c.id, r.last).catch(() => {})
  }

  /** At connect: which unread channels mention me (the newest page is enough to tell). */
  private async findMentions(channel: Id, after: Id) {
    const me = this.me?.id
    if (me === undefined) return
    try {
      const list = await api.messages(channel)
      const people = this.people()
      if (list.some((m) => m.id > after && m.authorId !== me && mentionsMe(m.content, people, me))) this.mentioned.add(channel)
    } catch {
      /* no dot is better than a wrong one */
    }
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
  async setAvatar(p: ProfilePatch) {
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
      if (typeof p.streamCursor === 'boolean') this.streamCursor = p.streamCursor
      if (typeof p.musicVolume === 'number') this.musicVolume = p.musicVolume
    } catch {
      // defaults are fine
    }
  }

  async boot() {
    void this.loadPrefs()
    // After the page reloads the call carries on underneath: pick up where it is.
    void api.voiceState?.().then((v) => (this.myVoice = v)).catch(() => {})
    setTimeout(() => (this.bootSlow = true), 4000)
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
    this.mobile.clear()
    this.lastSeen.clear()
    this.channels = []
    this.gotReady = false
    this.selectedId = null
    this.view = 'circle'
    this.messages.clear()
    this.reads.clear()
    this.mentioned.clear()
    this.hasMore.clear()
    this.loadError.clear()
    this.pending.clear()
    this.typing.clear()
    this.alerts = []
    this.voice = []
    this.myVoice = { channelId: null, muted: false, deafened: false }
    this.speaking.clear()
    this.voicePath = null
    this.security = null
    this.verifying = null
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
    this.reads.delete(id)
    this.mentioned.delete(id)
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

  /** I'm in `channel`'s call, but from another device (this one isn't connected to it). */
  inVoiceElsewhere(channel: Id): boolean {
    const me = this.me?.id
    return !this.inVoice(channel) && this.voice.some((m) => m.userId === me && m.channelId === channel)
  }

  joinVoice(channel: Id) {
    if (this.inVoice(channel)) return
    this.voicePath = null
    void this.voiceCall(() => api.voiceJoin(channel)).then(() => this.refreshSecurity())
  }

  /** Who in my call has which key status, and the call code. Null outside a call. */
  security = $state<CallSecurity | null>(null)
  /** The member whose person code is on screen, if any. */
  verifying = $state<Id | null>(null)
  private securitySeq = 0

  /** Re-read the call's verification data (on join and whenever the backend says it changed). */
  async refreshSecurity() {
    const seq = ++this.securitySeq
    try {
      const s = await api.e2eeCall()
      if (seq === this.securitySeq) this.security = s
    } catch {
      if (seq === this.securitySeq) this.security = null
    }
  }
  isLive(user: Id): boolean {
    return this.streams.some((s) => s.userId === user)
  }

  /** Show the mouse pointer in my Go Live; remembered for next time. */
  async setStreamCursor(on: boolean) {
    this.streamCursor = on
    try {
      const p = await api.prefsGet()
      await api.prefsSet({ ...p, streamCursor: on })
    } catch {
      // still applies to this stream
    }
  }

  async goLive(sourceId: string, preset: string): Promise<boolean> {
    this.goingLive = true
    try {
      await api.streamGoLive(sourceId, preset, this.streamCursor)
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
    this.securitySeq++
    this.security = null
    this.verifying = null
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
      case 'notice':
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
        this.mobile.clear()
        for (const id of m.d.mobile ?? []) this.mobile.add(id)
        this.lastSeen.clear()
        for (const l of m.d.lastSeen ?? []) this.lastSeen.set(l.userId, l.at)
        this.channels = m.d.channels
        this.reads.clear()
        this.mentioned.clear()
        for (const r of m.d.reads ?? []) {
          this.reads.set(r.channelId, { last: r.lastMessage ?? null, read: r.lastRead })
          if (r.lastMessage != null && r.lastMessage > r.lastRead) void this.findMentions(r.channelId, r.lastRead)
        }
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
      case 'messageCreated': {
        const r = this.reads.get(m.d.channelId) ?? { last: null, read: 0 }
        const mine = m.d.authorId === this.me?.id
        this.reads.set(m.d.channelId, { last: Math.max(r.last ?? 0, m.d.id), read: mine ? Math.max(r.read, m.d.id) : r.read })
        if (!mine && this.me && mentionsMe(m.d.content, this.people(), this.me.id)) this.mentioned.add(m.d.channelId)
        this.upsertMessage(m.d)
        this.typing.get(m.d.channelId)?.delete(m.d.authorId)
        return
      }
      case 'readUpdated': {
        const r = this.reads.get(m.d.channelId)
        if (r) this.reads.set(m.d.channelId, { ...r, read: Math.max(r.read, m.d.lastRead) })
        if (!this.isUnread(m.d.channelId)) this.mentioned.delete(m.d.channelId)
        return
      }
      case 'messageUpdated':
        this.upsertMessage(m.d)
        this.typing.get(m.d.channelId)?.delete(m.d.authorId)
        return
      case 'messageDeleted': {
        const list = this.messages.get(m.d.channelId)
        if (list) this.messages.set(m.d.channelId, list.filter((x) => x.id !== m.d.id))
        // The newest message went: the one before it is the newest now (as far as is known here).
        const r = this.reads.get(m.d.channelId)
        if (r && r.last === m.d.id) {
          const rest = this.messages.get(m.d.channelId)
          this.reads.set(m.d.channelId, { ...r, last: rest?.length ? rest[rest.length - 1].id : r.read })
        }
        return
      }
      case 'channelCreated':
        if (!this.channels.some((c) => c.id === m.d.id)) this.channels = [...this.channels, m.d]
        if (m.d.kind === 'text' && !this.reads.has(m.d.id)) this.reads.set(m.d.id, { last: null, read: 0 })
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
        this.mobile.delete(m.d.id)
        return
      case 'presence':
        if (m.d.online) this.online.add(m.d.userId)
        else this.online.delete(m.d.userId)
        if (m.d.online && m.d.mobile) this.mobile.add(m.d.userId)
        else this.mobile.delete(m.d.userId)
        if (m.d.lastSeen) this.lastSeen.set(m.d.userId, m.d.lastSeen)
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
