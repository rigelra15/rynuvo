import type { store as storeType } from './state.svelte'

type Store = typeof storeType

type Bridge = { callShow(room: string, muted: boolean, deafened: boolean): void; callHide(): void }

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
  return () => {
    const v = store.myVoice
    const room = v.channelId === null ? null : (store.channels.find((c) => c.id === v.channelId)?.name ?? 'Call')
    const key = room === null ? '' : JSON.stringify([room, v.muted, v.deafened])
    if (key === shown) return
    shown = key
    if (room === null) bridge.callHide()
    else bridge.callShow(room, v.muted, v.deafened)
  }
}
