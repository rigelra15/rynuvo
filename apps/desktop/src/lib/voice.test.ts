import { describe, expect, test } from 'vitest'
import { applyVoice, membersOf, type VoiceMember } from './voice'

const m = (userId: number, channelId: number, muted = false): VoiceMember => ({ userId, channelId, muted, deafened: false })

describe('applyVoice', () => {
  test('member update inserts and replaces', () => {
    let list = applyVoice([], { t: 'voiceMemberUpdated', d: m(1, 10) })
    list = applyVoice(list, { t: 'voiceMemberUpdated', d: m(2, 10) })
    list = applyVoice(list, { t: 'voiceMemberUpdated', d: m(1, 11, true) })
    expect(list).toHaveLength(2)
    expect(list.find((x) => x.userId === 1)).toEqual(m(1, 11, true))
    expect(membersOf(list, 10).map((x) => x.userId)).toEqual([2])
  })

  test('member left removes', () => {
    const list = applyVoice([m(1, 10), m(2, 10)], { t: 'voiceMemberLeft', d: { userId: 1, channelId: 10 } })
    expect(list).toEqual([m(2, 10)])
  })

  test('ready replaces all', () => {
    const list = applyVoice([m(1, 10)], {
      t: 'ready',
      d: { me: { id: 1, nickname: 'a', isAdmin: false }, users: [], channels: [], online: [], alerts: [], voice: [m(3, 12)] },
    })
    expect(list).toEqual([m(3, 12)])
  })

  test('channel deleted drops its members', () => {
    expect(applyVoice([m(1, 10), m(2, 11)], { t: 'channelDeleted', d: { id: 10 } })).toEqual([m(2, 11)])
  })
})
