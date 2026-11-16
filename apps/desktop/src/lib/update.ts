/** Update downloads automatically, then the application installs and relaunches after a short notice. */
export type UpdatePhase =
  | { phase: 'idle' }
  | { phase: 'downloading'; version: string }
  | { phase: 'ready'; version: string; seconds: number }
  | { phase: 'installing'; version: string }
  | { phase: 'failed'; version: string; error: string }

export const CHECK_EVERY_MS = 5 * 60 * 1000
export const NUDGE_GAP_MS = 60 * 1000

export function checkDue(last: number | null, now: number): boolean {
  return last === null || now - last >= NUDGE_GAP_MS
}

export interface TopBar {
  kind: 'outdated' | 'reconnecting' | 'update'
  canRestart: boolean
}

/** Update readiness no longer depends on voice or screen share activity. */
export function topBar(s: { outdated: boolean; online: boolean; busy: boolean; update: UpdatePhase }): TopBar | null {
  const ready = s.update.phase === 'ready'
  if (s.outdated) return { kind: 'outdated', canRestart: ready }
  if (!s.online) return { kind: 'reconnecting', canRestart: ready }
  if (s.update.phase !== 'idle') return { kind: 'update', canRestart: ready }
  return null
}

export type CheckResult = 'latest' | 'found' | 'failed'

export function updateStatus(s: { update: UpdatePhase; checking: boolean; result: CheckResult | null; busy: boolean }): { text: string | null; restart: boolean } {
  const u = s.update
  if (u.phase === 'ready') return { text: `${u.version} is ready. Restarting in ${u.seconds}s…`, restart: false }
  if (u.phase === 'installing') return { text: `Installing ${u.version} and restarting…`, restart: false }
  if (u.phase === 'failed') return { text: `Update failed: ${u.error}`, restart: false }
  if (u.phase === 'downloading') return { text: `Downloading ${u.version} in the background…`, restart: false }
  if (s.checking) return { text: 'Looking for a new version…', restart: false }
  if (s.result === 'latest') return { text: "You're on the latest version.", restart: false }
  if (s.result === 'failed') return { text: "Couldn't reach the update server. Try again in a bit.", restart: false }
  return { text: null, restart: false }
}
