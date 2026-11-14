/** Avatar colours and icons. Keys must match rynuvo_proto::AVATAR_COLORS / AVATAR_ICONS. */
export const PALETTE = {
  violet: '#8b5cf6',
  blue: '#3b82f6',
  sky: '#0ea5e9',
  teal: '#14b8a6',
  green: '#22c55e',
  lime: '#65a30d',
  amber: '#d97706',
  orange: '#f97316',
  red: '#ef4444',
  pink: '#ec4899',
  fuchsia: '#d946ef',
  slate: '#64748b',
} as const
export type ColorKey = keyof typeof PALETTE
const KEYS = Object.keys(PALETTE) as ColorKey[]

export const AVATAR_ICONS = [
  'cat', 'dog', 'rabbit', 'bird', 'fish', 'turtle', 'ghost', 'skull', 'rocket', 'gamepad', 'headphones', 'music',
  'coffee', 'pizza', 'flame', 'zap', 'star', 'moon', 'sun', 'leaf', 'flower', 'crown', 'anchor', 'camera',
] as const
export type AvatarIcon = (typeof AVATAR_ICONS)[number]

export function isAvatarIcon(s: string | null | undefined): s is AvatarIcon {
  return !!s && (AVATAR_ICONS as readonly string[]).includes(s)
}

/** The user's colour: their pick, else one derived from their id (same on every client). */
export function avatarColor(u: { id: number; avatarColor?: string | null }): string {
  const k = u.avatarColor
  if (k && k in PALETTE) return PALETTE[k as ColorKey]
  return PALETTE[KEYS[((u.id % KEYS.length) + KEYS.length) % KEYS.length]]
}
