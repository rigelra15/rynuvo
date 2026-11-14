import { describe, expect, test } from 'vitest'
import { findMentions, mentionQuery, mentionsMe, suggest } from './mention'

const users = [
  { id: 1, nickname: 'Rigel' },
  { id: 2, nickname: 'Budi' },
  { id: 3, nickname: 'Budi Santoso' },
  { id: 4, nickname: 'ana' },
]

describe('findMentions', () => {
  test('finds @nick case-insensitively, preferring the longest name', () => {
    const t = 'hi @budi santoso and @RIGEL!'
    expect(findMentions(t, users)).toEqual([
      { start: 3, end: 16, id: 3 },
      { start: 21, end: 27, id: 1 },
    ])
  })
  test('needs a word boundary on both sides', () => {
    expect(findMentions('mail@rigel.dev', users)).toEqual([])
    expect(findMentions('@rigelx', users)).toEqual([])
    expect(findMentions('@Budi, ok', users)).toEqual([{ start: 0, end: 5, id: 2 }])
  })
  test('@everyone mentions everybody', () => {
    expect(findMentions('@everyone game?', users)).toEqual([{ start: 0, end: 9, id: 'everyone' }])
  })
})

describe('mentionsMe', () => {
  test('true for my name or @everyone, not for someone else', () => {
    expect(mentionsMe('yo @rigel', users, 1)).toBe(true)
    expect(mentionsMe('@everyone', users, 1)).toBe(true)
    expect(mentionsMe('yo @budi', users, 1)).toBe(false)
  })
})

describe('mentionQuery', () => {
  test('the @word being typed at the caret', () => {
    expect(mentionQuery('hey @bu', 7)).toEqual({ start: 4, query: 'bu' })
    expect(mentionQuery('@', 1)).toEqual({ start: 0, query: '' })
    expect(mentionQuery('hey @budi s', 11)).toEqual({ start: 4, query: 'budi s' })
  })
  test('none inside an email, across a new line, or when not typing a mention', () => {
    expect(mentionQuery('a@b', 3)).toBeNull()
    expect(mentionQuery('@bu\nx', 5)).toBeNull()
    expect(mentionQuery('hello', 5)).toBeNull()
  })
})

describe('suggest', () => {
  test('members whose name starts with the query, without me, then @everyone', () => {
    expect(suggest('bu', users, 1).map((s) => s.nickname)).toEqual(['Budi', 'Budi Santoso'])
    expect(suggest('', users, 1).map((s) => s.nickname)).toEqual(['ana', 'Budi', 'Budi Santoso', 'everyone'])
    expect(suggest('ev', users, 1).map((s) => s.nickname)).toEqual(['everyone'])
  })
})
