/** Pure helpers for the message list: who-said-what runs and day dividers. */
type Msg = { authorId: number; createdAt: number }

/** A run of messages from one person breaks after this gap. */
export const GROUP_MS = 5 * 60 * 1000

/** Does `cur` start a new run (show name/avatar)? */
export function isHead(prev: Msg | undefined, cur: Msg): boolean {
  return !prev || prev.authorId !== cur.authorId || cur.createdAt - prev.createdAt > GROUP_MS
}

const dayKey = (ms: number) => new Date(ms).toDateString()

/** Is `cur` the first message of a local calendar day? */
export function newDay(prev: Msg | undefined, cur: Msg): boolean {
  return !prev || dayKey(prev.createdAt) !== dayKey(cur.createdAt)
}

const MONTHS = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec']

/** "Today", "Yesterday", "12 Nov", or "12 Nov 2025" for another year. */
export function dayLabel(ms: number, now: number): string {
  const d = new Date(ms)
  const today = new Date(now)
  if (dayKey(ms) === today.toDateString()) return 'Today'
  const y = new Date(now)
  y.setDate(y.getDate() - 1)
  if (dayKey(ms) === y.toDateString()) return 'Yesterday'
  const s = `${d.getDate()} ${MONTHS[d.getMonth()]}`
  return d.getFullYear() === today.getFullYear() ? s : `${s} ${d.getFullYear()}`
}
