/** Pure pieces of the Home page: the "Active Now" cards and each friend's status line. */
type Ch = { id: number; name: string }
type Vm = { userId: number; channelId: number }

export interface ActiveCard { channelId: number; channel: string; userIds: number[]; title: string; live: number[] }

/** "A", "A and B", "A, B and 2 others". */
export function groupTitle(names: string[]): string {
  if (names.length <= 1) return names[0] ?? ''
  if (names.length === 2) return `${names[0]} and ${names[1]}`
  const rest = names.length - 2
  return `${names[0]}, ${names[1]} and ${rest} ${rest === 1 ? 'other' : 'others'}`
}

/** One card per voice channel with people in it, the busiest first. */
export function activeNow(voice: readonly Vm[], channels: readonly Ch[], nick: (id: number) => string, live: ReadonlySet<number>): ActiveCard[] {
  const byName = (a: number, b: number) => nick(a).localeCompare(nick(b), undefined, { sensitivity: 'base' })
  return channels
    .map((c) => {
      const userIds = voice.filter((v) => v.channelId === c.id).map((v) => v.userId).sort(byName)
      return { channelId: c.id, channel: c.name, userIds, title: groupTitle(userIds.map(nick)), live: userIds.filter((id) => live.has(id)) }
    })
    .filter((c) => c.userIds.length > 0)
    .sort((a, b) => b.userIds.length - a.userIds.length)
}

/** The status line under a friend's name. */
export function presence(id: number, online: ReadonlySet<number>, voice: readonly Vm[], channels: readonly Ch[], live: ReadonlySet<number>): string {
  const v = voice.find((m) => m.userId === id)
  const where = v ? channels.find((c) => c.id === v.channelId)?.name : undefined
  if (where && live.has(id)) return `Streaming in ${where}`
  if (where) return `In voice · ${where}`
  return online.has(id) ? 'Online' : 'Offline'
}
