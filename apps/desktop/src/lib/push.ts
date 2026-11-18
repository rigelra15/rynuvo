// Android: notifications while Rynuvo is closed (Firebase push, see PushService.kt).
// - signs this phone up with its Firebase token once signed in (and again when the token changes)
// - tells the server when the app goes to the background, so it pushes to this phone meanwhile
// - opens the chat or room a tapped notification is about
import { api } from './api'
import { isAndroid } from './platform'
import type { Id } from './types'

interface PushBridge {
  pushToken(): string
  pushSetup(server: string, key: string): void
  pushClear(): void
  takeOpen(): number
}
const bridge = () => (window as unknown as { RynuvoAndroid?: Partial<PushBridge> }).RynuvoAndroid
type W = { __rynuvoPushToken?: (t: string) => void; __rynuvoOpen?: (ch: number) => void }

async function signUp(token: string) {
  if (!token) return
  try {
    const { server, key } = await api.pushRegister(token)
    bridge()?.pushSetup?.(server, key)
  } catch (e) {
    console.warn('push sign-up failed', e)
  }
}

/** Once signed in. `open` goes to a chat or room. Returns a stop function. */
export function startPush(open: (channel: Id) => void): () => void {
  const b = bridge()
  if (!isAndroid || !b?.pushToken) return () => {}
  const w = window as unknown as W
  w.__rynuvoPushToken = (t) => void signUp(t)
  void signUp(b.pushToken())

  const away = () => void api.setAway(document.visibilityState === 'hidden').catch(() => {})
  document.addEventListener('visibilitychange', away)

  w.__rynuvoOpen = (ch) => {
    b.takeOpen?.()
    open(ch)
  }
  const first = b.takeOpen?.() ?? 0
  if (first) open(first)

  return () => {
    document.removeEventListener('visibilitychange', away)
    delete w.__rynuvoPushToken
    delete w.__rynuvoOpen
  }
}

/** Signing out: this phone stops getting notifications for that account. */
export async function stopPush() {
  if (!isAndroid) return
  bridge()?.pushClear?.()
  await api.pushUnregister().catch(() => {})
}
