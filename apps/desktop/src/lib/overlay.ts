// Desktop: the call overlay (a see-through window over a game, like Discord's). This side, in the
// main window, decides when it shows (in a call, with a game in front, as the app reports) and
// tells it what to draw; its buttons come back here, where the call is.
import { invoke } from '@tauri-apps/api/core'
import { emitTo, listen } from '@tauri-apps/api/event'
import type { store as storeType } from './state.svelte'
import type { ImageRef, User } from './types'

type Store = typeof storeType

export const OVERLAY = 'overlay'
export const STATE_EVENT = 'rynuvo://overlay-state'
export const ACTION_EVENT = 'rynuvo://overlay-action'
export const READY_EVENT = 'rynuvo://overlay-ready'
export const CHAT_EVENT = 'rynuvo://overlay-chat'
/** From the app: a game came to the front (true) or went. */
const GAME_EVENT = 'rynuvo://game'
const PREF = 'rynuvo.overlay'

export interface OverlayPerson { id: number; name: string; user: User | null; muted: boolean; deafened: boolean; talking: boolean; me: boolean }
export interface OverlayState { muted: boolean; deafened: boolean; people: OverlayPerson[] }
/** A chat message that came in while the overlay is up. */
export interface OverlayChat { id: number; name: string; user: User | null; authorId: number; channel: string; text: string; image: ImageRef | null }
export type OverlayAction = 'mute' | 'deafen' | 'leave'

/** Whether the overlay is wanted (on unless turned off in Settings). */
export function overlayEnabled(): boolean {
  try {
    return localStorage.getItem(PREF) !== '0'
  } catch {
    return true
  }
}

export function setOverlayEnabled(on: boolean) {
  try {
    localStorage.setItem(PREF, on ? '1' : '0')
  } catch {
    // Private storage off: it stays on for this run.
  }
  window.dispatchEvent(new Event(PREF))
}

/** Starts it; returns the function to call whenever the call changes. */
export function overlay(store: Store): () => void {
  let gaming = false
  let enabled = overlayEnabled()
  let shown = false
  let sent = ''
  let last: OverlayState | null = null

  const send = (force = false) => {
    if (!last) return
    const s = JSON.stringify(last)
    if (!force && s === sent) return
    sent = s
    void emitTo(OVERLAY, STATE_EVENT, last)
  }

  const show = () => {
    const want = enabled && gaming && last !== null
    if (want === shown) return
    shown = want
    void invoke('overlay_show', { show: want }).catch(() => (shown = !want))
  }

  // A game that leaves the front only for a moment (a notification, a quick alt-tab) keeps it.
  let away: ReturnType<typeof setTimeout> | undefined
  void listen<boolean>(GAME_EVENT, ({ payload }) => {
    clearTimeout(away)
    if (payload) {
      gaming = true
      show()
    } else {
      away = setTimeout(() => {
        gaming = false
        show()
      }, 2500)
    }
  })
  window.addEventListener(PREF, () => {
    enabled = overlayEnabled()
    show()
  })
  void listen(READY_EVENT, () => send(true))
  // New messages pop up on it, and count as read: they were seen there.
  store.onMessage = (m) => {
    if (!shown) return
    const c = store.channels.find((x) => x.id === m.channelId)
    if (!c || c.kind !== 'text') return
    const chat: OverlayChat = { id: m.id, name: store.nick(m.authorId), user: store.user(m.authorId) ?? null, authorId: m.authorId, channel: c.name, text: m.content, image: m.image }
    void emitTo(OVERLAY, CHAT_EVENT, chat)
    store.readUpTo(m.channelId, m.id)
  }
  void listen<OverlayAction>(ACTION_EVENT, ({ payload }) => {
    if (!store.myVoice.channelId) return
    if (payload === 'mute') store.toggleMute()
    else if (payload === 'deafen') store.toggleDeafen()
    else if (payload === 'leave') store.leaveVoice()
  })

  return () => {
    const v = store.myVoice
    if (v.channelId === null) {
      last = null
      sent = ''
      show()
      return
    }
    const me = store.me?.id ?? 0
    const people = store.voice
      .filter((m) => m.channelId === v.channelId)
      .map((m) => ({
        id: m.userId,
        name: store.nick(m.userId),
        user: store.user(m.userId) ?? null,
        muted: m.userId === me ? v.muted : m.muted,
        deafened: m.userId === me ? v.deafened : m.deafened,
        talking: store.speaking.has(m.userId),
        me: m.userId === me,
      }))
    last = { muted: v.muted, deafened: v.deafened, people }
    send()
    show()
  }
}
