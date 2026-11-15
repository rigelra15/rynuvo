/** Voice room seats: round avatars with a name under each, as big as the room allows (up to `max`,
 *  never under `min`). Ties go to fewer rows, then fewer columns, so a few people sit in one line. */
export interface Seats { cols: number; size: number }
const LABEL = 28

export function seats(n: number, W: number, H: number, gap: number, max = 168, min = 48): Seats {
  if (n <= 0) return { cols: 1, size: 0 }
  let best = { cols: 1, rows: n, size: -1 }
  for (let cols = 1; cols <= n; cols++) {
    const rows = Math.ceil(n / cols)
    const byW = (W - gap * (cols - 1)) / cols
    const byH = (H - gap * (rows - 1)) / rows - LABEL
    const size = Math.floor(Math.max(min, Math.min(max, byW, byH)))
    if (size > best.size || (size === best.size && rows < best.rows)) best = { cols, rows, size }
  }
  return { cols: best.cols, size: best.size }
}
