// How long a voice channel's call has been going.
import type { VoiceMember } from './voice'

/** When the call started: when someone joined the empty channel (server ms), or null if unknown. */
export function callStart(members: VoiceMember[]): number | null {
  const times = members.map((m) => m.callSince || m.joinedAt || 0).filter((t) => t > 0)
  return times.length ? Math.min(...times) : null
}
