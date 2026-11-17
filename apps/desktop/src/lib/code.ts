/** Invite and transfer codes: twelve characters shown as `ABCD-EFGH-JKLM`. */
const LEN = 12

/** What the user typed or pasted, as the code should look: uppercase, a dash every four. */
export function formatCode(raw: string): string {
  const chars = raw.toUpperCase().replace(/[^A-Z0-9]/g, '').slice(0, LEN)
  return chars.match(/.{1,4}/g)?.join('-') ?? ''
}

const PREFIX = 'rynuvo:transfer:'

/** The text a transfer code's QR carries. */
export const toQr = (code: string) => PREFIX + formatCode(code)

/** The transfer code in a scanned QR (or a bare code), or null when it isn't one. */
export function fromQr(text: string): string | null {
  const t = text.trim()
  const body = t.toLowerCase().startsWith(PREFIX) ? t.slice(PREFIX.length) : t
  if (/[^A-Za-z0-9\s-]/.test(body)) return null
  const code = formatCode(body)
  return code.length === LEN + 2 ? code : null
}
