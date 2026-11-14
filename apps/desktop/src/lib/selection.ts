import type { Channel, Id } from './types'

/**
 * Which text channel to show after `deletedId` is removed.
 * Keeps `current` if it still exists; otherwise the nearest remaining text channel by position.
 */
export function next(channels: Channel[], current: Id | null, deletedId: Id): Id | null {
  const text = channels.filter((c) => c.kind === 'text').sort((a, b) => a.position - b.position || a.id - b.id)
  if (current !== null && current !== deletedId && text.some((c) => c.id === current)) return current
  const idx = text.findIndex((c) => c.id === deletedId)
  const rest = text.filter((c) => c.id !== deletedId)
  if (rest.length === 0) return null
  if (idx <= 0) return rest[0].id
  return rest[Math.min(idx - 1, rest.length - 1)].id
}
