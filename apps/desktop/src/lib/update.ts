/** Update downloads automatically, then the application installs and relaunches after a short notice. */
export type UpdatePhase =
  | { phase: 'idle' }
  /** `progress` is 0–1, or null while the size isn't known. */
  | { phase: 'downloading'; version: string; progress?: number | null }
  | { phase: 'ready'; version: string; seconds: number }
  /** Android: downloaded and verified, waiting on the system installer (the user confirms there). */
  | { phase: 'install'; version: string; needsPermission?: boolean }
  | { phase: 'installing'; version: string }
  | { phase: 'failed'; version: string; error: string }

export const CHECK_EVERY_MS = 5 * 60 * 1000
export const NUDGE_GAP_MS = 60 * 1000
/** How long the splash waits to hear whether there's a new version before opening the app anyway. */
export const BOOT_WAIT_MS = 3500

/** Download progress as 0–1 from bytes so far and the full size (null when the size is unknown). */
export function progressOf(received: number, total: number | null | undefined): number | null {
  if (!total || total <= 0) return null
  return Math.min(1, Math.max(0, received / total))
}

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
  if (u.phase === 'install') return { text: `${u.version} is downloaded and ready to install.`, restart: false }
  if (u.phase === 'downloading') {
    const pct = u.progress == null ? '' : ` ${Math.round(u.progress * 100)}%`
    return { text: `Downloading ${u.version} in the background…${pct}`, restart: false }
  }
  if (s.checking) return { text: 'Looking for a new version…', restart: false }
  if (s.result === 'latest') return { text: "You're on the latest version.", restart: false }
  if (s.result === 'failed') return { text: "Couldn't reach the update server. Try again in a bit.", restart: false }
  return { text: null, restart: false }
}
