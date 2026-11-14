/** How the app updates itself: download in the background, restart only when the user says so. */
export type UpdatePhase =
  | { phase: 'idle' }
  | { phase: 'downloading'; version: string }
  | { phase: 'ready'; version: string }

export const CHECK_EVERY_MS = 6 * 60 * 60 * 1000

export interface Banner {
  text: string
  canRestart: boolean
  hint: string
}

/** `busy`: in voice or live, where a restart would cut everyone off. */
export function banner(s: UpdatePhase, busy: boolean): Banner | null {
  if (s.phase !== 'ready') return null
  return { text: `Update ready (${s.version})`, canRestart: !busy, hint: busy ? 'Leave voice to restart' : '' }
}
