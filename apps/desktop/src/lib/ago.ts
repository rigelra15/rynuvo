/** How long ago someone was last here, as a short chip: 1m, 45m, 3h, 2d, 3w, 5mo, 2y. */
export function ago(at: number, now: number): string {
  const m = Math.max(1, Math.floor((now - at) / 60_000))
  if (m < 60) return `${m}m`
  const h = Math.floor(m / 60)
  if (h < 24) return `${h}h`
  const d = Math.floor(h / 24)
  if (d < 7) return `${d}d`
  if (d < 30) return `${Math.floor(d / 7)}w`
  if (d < 365) return `${Math.floor(d / 30)}mo`
  return `${Math.floor(d / 365)}y`
}
