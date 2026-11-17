// What the app asks Android for, as the welcome screens show it.
// - mic: voice rooms (the mic is read from native code, which can't ask by itself)
// - notifications: the call controls in the notification bar
// - install: letting Rynuvo install its own updates ("install unknown apps")

export type Perm = 'mic' | 'notifications' | 'install'
/** granted · ask (Android's dialog can come up) · blocked (only Settings can turn it on). */
export type PermState = 'granted' | 'ask' | 'blocked'

interface PermBridge {
  permission(name: string): string
  askPermission(name: string): void
  openAppSettings(): void
  canInstall(): boolean
  allowInstall(): void
}
const bridge = () => (window as unknown as { RynuvoAndroid?: Partial<PermBridge> }).RynuvoAndroid

const WELCOMED = 'rynuvo.welcomed'

/** The welcome screens have been through once on this device. */
export function welcomed(): boolean {
  try {
    return localStorage.getItem(WELCOMED) === '1'
  } catch {
    return false
  }
}

export function markWelcomed() {
  try {
    localStorage.setItem(WELCOMED, '1')
  } catch {
    // Private storage off: the welcome shows again next launch, which is harmless.
  }
}

class Permissions {
  state = $state<Record<Perm, PermState>>({ mic: 'ask', notifications: 'ask', install: 'ask' })

  /** Whether this build can ask at all (the Android bridge is there). */
  get available() {
    return !!bridge()?.permission
  }

  refresh() {
    const b = bridge()
    if (!b?.permission) return
    const read = (n: string): PermState => {
      const s = b.permission!(n)
      return s === 'granted' || s === 'blocked' ? s : 'ask'
    }
    const next: Record<Perm, PermState> = {
      mic: read('mic'),
      notifications: read('notifications'),
      install: b.canInstall?.() === false ? 'ask' : 'granted',
    }
    // Only touch the state when something changed, so a poll doesn't wake every effect.
    if ((Object.keys(next) as Perm[]).some((k) => next[k] !== this.state[k])) this.state = next
  }

  allGranted() {
    this.refresh()
    return Object.values(this.state).every((s) => s === 'granted')
  }

  /** Ask for one: Android's dialog, or its settings page when the dialog won't come up any more. */
  ask(p: Perm) {
    const b = bridge()
    if (!b) return
    if (p === 'install') return b.allowInstall?.()
    if (this.state[p] === 'blocked') return b.openAppSettings?.()
    b.askPermission?.(p)
  }

  constructor() {
    // Android answers a dialog through here (MainActivity.answer).
    ;(window as unknown as { __rynuvoPerm?: (n: string, s: string) => void }).__rynuvoPerm = () => this.refresh()
  }
}

export const permissions = new Permissions()
