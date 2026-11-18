import type { store as storeType } from './state.svelte'
import { avatarColor } from './avatar'

type Store = typeof storeType

type Bridge = {
  callShow(room: string, muted: boolean, deafened: boolean): void
  callHide(): void
  /** The floating call bubble (newer apps only). */
  callBubble?(json: string): void
  callSpeaking?(ids: string): void
}

/** Android: while I'm in a call, the call notification (Mute, Deafen, End call) keeps the app going
 *  in the background. Its buttons come back here. Returns a function to keep it in step. */
export function callNotice(store: Store): (() => void) | null {
  const android = (window as { RynuvoAndroid?: Partial<Bridge> }).RynuvoAndroid
  if (!android?.callShow || !android.callHide) return null
  const bridge = android as Bridge
  ;(window as { __rynuvoCall?: (a: string) => void }).__rynuvoCall = (a) => {
    if (!store.myVoice.channelId) return
    if (a === 'mute') store.toggleMute()
    else if (a === 'deafen') store.toggleDeafen()
    else if (a === 'leave') store.leaveVoice()
  }
  let shown = ''
  let bubbled = ''
  let talking = ''
  return () => {
    const v = store.myVoice
    const room = v.channelId === null ? null : (store.channels.find((c) => c.id === v.channelId)?.name ?? 'Call')
    const key = room === null ? '' : JSON.stringify([room, v.muted, v.deafened])
    if (key !== shown) {
      shown = key
      if (room === null) bridge.callHide()
      else bridge.callShow(room, v.muted, v.deafened)
    }
    if (room === null) {
      bubbled = ''
      talking = ''
      return
    }
    // The bubble: who's in the room, as it draws them (faces are fetched natively by user id).
    const people = store.voice
      .filter((m) => m.channelId === v.channelId)
      .map((m) => {
        const u = store.user(m.userId)
        return { id: m.userId, name: store.nick(m.userId), avatar: u?.avatarImage ?? null, color: avatarColor(u ?? { id: m.userId }), muted: m.muted, deafened: m.deafened }
      })
    const b = JSON.stringify({ room, muted: v.muted, deafened: v.deafened, me: store.me?.id ?? 0, people })
    if (b !== bubbled) {
      bubbled = b
      bridge.callBubble?.(b)
    }
    const t = [...store.speaking].join(',')
    if (t !== talking) {
      talking = t
      bridge.callSpeaking?.(t)
    }
  }
}
