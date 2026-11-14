/** The member list's two sections, each sorted by name. */
type Member = { id: number; nickname: string }

export function memberGroups<U extends Member>(users: Iterable<U>, online: ReadonlySet<number>, meId?: number) {
  const byName = (a: U, b: U) => a.nickname.localeCompare(b.nickname, undefined, { sensitivity: 'base' })
  const on: U[] = []
  const off: U[] = []
  for (const u of users) (online.has(u.id) || u.id === meId ? on : off).push(u)
  return { online: on.sort(byName), offline: off.sort(byName) }
}
