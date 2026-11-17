/** A picture's colour: the most vivid hue it has plenty of, set to a shade that reads like the
 *  palette's (not too dark, not too pale), as `#rrggbb`. */
export function dominantColor(px: Uint8ClampedArray): string {
  const BINS = 24
  const w = new Float64Array(BINS)
  const sum = Array.from({ length: BINS }, () => [0, 0, 0])
  let grey = [0, 0, 0, 0]
  for (let i = 0; i + 3 < px.length; i += 4) {
    if (px[i + 3] < 128) continue
    const [r, g, b] = [px[i], px[i + 1], px[i + 2]]
    const [h, s, l] = toHsl(r, g, b)
    // Vivid, mid-light pixels count most; washed-out or near-black ones barely.
    const weight = s * (1 - Math.abs(2 * l - 1))
    if (weight < 0.08) {
      grey = [grey[0] + r, grey[1] + g, grey[2] + b, grey[3] + 1]
      continue
    }
    const k = Math.floor((h / 360) * BINS) % BINS
    w[k] += weight
    sum[k][0] += r * weight
    sum[k][1] += g * weight
    sum[k][2] += b * weight
  }
  let best = 0
  for (let k = 1; k < BINS; k++) if (w[k] > w[best]) best = k
  // Hardly any colour at all (a black-and-white photo): a calm slate-like grey from the picture.
  if (w[best] === 0) {
    const n = grey[3] || 1
    const [h, s] = toHsl(grey[0] / n, grey[1] / n, grey[2] / n)
    return toHex(h, Math.min(s, 0.18), 0.45)
  }
  const [h, s, l] = toHsl(sum[best][0] / w[best], sum[best][1] / w[best], sum[best][2] / w[best])
  return toHex(h, Math.max(s, 0.5), Math.min(0.58, Math.max(0.42, l)))
}

/** The colour of an image (a Blob or a media URL), from a small copy of it. */
export async function imageColor(src: Blob | string): Promise<string> {
  const blob = typeof src === 'string' ? await (await fetch(src)).blob() : src
  const bmp = await createImageBitmap(blob, { resizeWidth: 48, resizeHeight: 48, resizeQuality: 'medium' })
  const c = document.createElement('canvas')
  c.width = c.height = 48
  const g = c.getContext('2d', { willReadFrequently: true })!
  g.drawImage(bmp, 0, 0, 48, 48)
  bmp.close()
  return dominantColor(g.getImageData(0, 0, 48, 48).data)
}

function toHsl(r: number, g: number, b: number): [number, number, number] {
  r /= 255
  g /= 255
  b /= 255
  const max = Math.max(r, g, b)
  const min = Math.min(r, g, b)
  const l = (max + min) / 2
  const d = max - min
  if (d === 0) return [0, 0, l]
  const s = d / (1 - Math.abs(2 * l - 1))
  const h = max === r ? ((g - b) / d + 6) % 6 : max === g ? (b - r) / d + 2 : (r - g) / d + 4
  return [h * 60, s, l]
}

function toHex(h: number, s: number, l: number): string {
  const a = s * Math.min(l, 1 - l)
  const f = (n: number) => {
    const k = (n + h / 30) % 12
    return Math.round(255 * (l - a * Math.max(-1, Math.min(k - 3, 9 - k, 1))))
  }
  return '#' + [f(0), f(8), f(4)].map((v) => v.toString(16).padStart(2, '0')).join('')
}
