/** How the app updates itself: download in the background, restart only when the user says so. */
export type UpdatePhase =
  | { phase: 'idle' }
  | { phase: 'downloading'; version: string }
  | { phase: 'ready'; version: string }

export const CHECK_EVERY_MS = 6 * 60 * 60 * 1000


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
