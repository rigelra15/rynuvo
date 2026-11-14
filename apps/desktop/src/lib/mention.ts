/** @mentions are plain text ("@Budi"), so older clients still read them naturally. */
type Member = { id: number; nickname: string }
export type MentionId = number | 'everyone'
export interface Mention { start: number; end: number; id: MentionId }

export const EVERYONE = 'everyone' as const
const WORD = /[\p{L}\p{N}_]/u
const isWord = (c: string | undefined) => c !== undefined && WORD.test(c)

/** Every @mention of a known member (longest name wins) or @everyone in `text`. */
export function findMentions(text: string, users: readonly Member[]): Mention[] {
  const names: { id: MentionId; lower: string }[] = [
    ...users.map((u) => ({ id: u.id as MentionId, lower: u.nickname.toLowerCase() })),
    { id: EVERYONE, lower: EVERYONE },
  ].sort((a, b) => b.lower.length - a.lower.length)
  const lower = text.toLowerCase()
  const out: Mention[] = []
  for (let i = lower.indexOf('@'); i !== -1; i = lower.indexOf('@', i + 1)) {
    if (isWord(text[i - 1])) continue
    const hit = names.find((n) => lower.startsWith(n.lower, i + 1) && !isWord(text[i + 1 + n.lower.length]))
    if (!hit) continue
    const end = i + 1 + hit.lower.length
    out.push({ start: i, end, id: hit.id })
    i = end - 1
  }
  return out
}

export function mentionsMe(text: string, users: readonly Member[], me: number): boolean {
  return findMentions(text, users).some((m) => m.id === me || m.id === EVERYONE)
}

/** The mention being typed just before the caret: "@" at a word start, no line break, ≤ 32 chars. */
export function mentionQuery(text: string, caret: number): { start: number; query: string } | null {
  const before = text.slice(0, caret)
  const at = before.lastIndexOf('@')
  if (at === -1 || isWord(before[at - 1])) return null
  const query = before.slice(at + 1)
  if (query.includes('\n') || [...query].length > 32) return null
  return { start: at, query }
}

/** Autocomplete: members (not me) whose name starts with `query`, by name, then @everyone. */
export function suggest(query: string, users: readonly Member[], me: number | undefined, limit = 8): Member[] {
  const q = query.toLowerCase()
  const list = users
    .filter((u) => u.id !== me && u.nickname.toLowerCase().startsWith(q))
    .sort((a, b) => a.nickname.localeCompare(b.nickname, undefined, { sensitivity: 'base' }))
  if (EVERYONE.startsWith(q)) list.push({ id: -1, nickname: EVERYONE })
  return list.slice(0, limit)
}
