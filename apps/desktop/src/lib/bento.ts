/** Voice "bento" grid: the column count whose 16:9 tiles come out biggest in a `W×H` stage. */
export interface Bento { cols: number; rows: number; w: number; h: number }

export function bento(n: number, W: number, H: number, gap: number, ratio = 16 / 9): Bento {
  if (n <= 0) return { cols: 1, rows: 0, w: 0, h: 0 }
  let best: Bento = { cols: 1, rows: n, w: 0, h: 0 }
  for (let cols = 1; cols <= n; cols++) {
    const rows = Math.ceil(n / cols)
    const maxW = (W - gap * (cols - 1)) / cols
    const maxH = (H - gap * (rows - 1)) / rows
    if (maxW <= 0 || maxH <= 0) continue
    const w = Math.min(maxW, maxH * ratio)
    if (w > best.w) best = { cols, rows, w, h: w / ratio }
  }
  return { ...best, w: Math.floor(best.w), h: Math.floor(best.h) }
}
