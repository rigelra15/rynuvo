/** How the app updates itself: download in the background, restart only when the user says so. */
export type UpdatePhase =
  | { phase: 'idle' }
  | { phase: 'downloading'; version: string }
  | { phase: 'ready'; version: string }

/** Asking is one tiny file on our own server, so ask often: a new version shows up within minutes. */
export const CHECK_EVERY_MS = 5 * 60 * 1000
/** Coming back to the app (focus, reconnect) asks again, at most this often. */
export const NUDGE_GAP_MS = 60 * 1000

export function checkDue(last: number | null, now: number): boolean {
  return last === null || now - last >= NUDGE_GAP_MS
}


export interface TopBar {
  kind: 'outdated' | 'reconnecting' | 'update'
  canRestart: boolean
}

/** The one bar above the app. A ready update can always be installed once the server has refused
 *  this version (`outdated`): there is no call left to interrupt. */
export function topBar(s: { outdated: boolean; online: boolean; busy: boolean; update: UpdatePhase }): TopBar | null {
  const ready = s.update.phase === 'ready'
  if (s.outdated) return { kind: 'outdated', canRestart: ready }
  if (!s.online) return { kind: 'reconnecting', canRestart: ready && !s.busy }
  if (ready) return { kind: 'update', canRestart: !s.busy }
  return null
}

export type CheckResult = 'latest' | 'found' | 'failed'

/** The line under "Check for updates" in Settings, and whether a restart can happen right now. */
export function updateStatus(s: { update: UpdatePhase; checking: boolean; result: CheckResult | null; busy: boolean }): { text: string | null; restart: boolean } {
  if (s.update.phase === 'ready') {
    return s.busy ? { text: `${s.update.version} is ready. Leave voice to restart.`, restart: false } : { text: `${s.update.version} is ready.`, restart: true }
  }
  if (s.update.phase === 'downloading') return { text: `Downloading ${s.update.version} in the background…`, restart: false }
  if (s.checking) return { text: 'Looking for a new version…', restart: false }
  if (s.result === 'latest') return { text: "You're on the latest version.", restart: false }
  if (s.result === 'failed') return { text: "Couldn't reach the update server. Try again in a bit.", restart: false }
  return { text: null, restart: false }
}
