/** Avatar colours and icons. Keys must match rynuvo_proto::AVATAR_COLORS / AVATAR_ICONS. */
export const PALETTE = {
  violet: '#8b5cf6',
  blue: '#3b82f6',
  sky: '#0c8ec8',
  teal: '#109788',
  green: '#1b9a49',
  lime: '#5d960c',
  amber: '#ca6f06',
  orange: '#d96413',
  red: '#ef4444',
  pink: '#e74796',
  fuchsia: '#d244e8',
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

/** The colour for a name written on a dark bubble: the avatar colour lifted toward white so it stays readable (≥ 4.5:1). */
export function nameColor(hex: string): string {
  const mix = (i: number) => Math.round(parseInt(hex.slice(i, i + 2), 16) * 0.7 + 255 * 0.3)
  return '#' + [1, 3, 5].map((i) => mix(i).toString(16).padStart(2, '0')).join('')
}
