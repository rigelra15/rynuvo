import { describe, expect, it } from 'vitest'
import { formatCode, fromQr, toQr } from './code'

describe('formatCode', () => {
  it('uppercases and adds a dash every four characters', () => {
    expect(formatCode('abcd')).toBe('ABCD')
    expect(formatCode('abcde')).toBe('ABCD-E')
    expect(formatCode('abcdefghjklm')).toBe('ABCD-EFGH-JKLM')
  })
  it('drops spaces, stray dashes and symbols', () => {
    expect(formatCode(' ab-cd  ef/gh ')).toBe('ABCD-EFGH')
    expect(formatCode('ABCD--EFGH-')).toBe('ABCD-EFGH')
  })
  it('stops at twelve characters', () => {
    expect(formatCode('abcdefghjklmnpq')).toBe('ABCD-EFGH-JKLM')
  })
  it('keeps a pasted code as is', () => {
    expect(formatCode('ABCD-EFGH-JKLM')).toBe('ABCD-EFGH-JKLM')
  })
})

describe('QR payload', () => {
  it('round-trips a transfer code', () => {
    expect(fromQr(toQr('ABCD-EFGH-JKLM'))).toBe('ABCD-EFGH-JKLM')
  })
  it('accepts a bare code', () => {
    expect(fromQr('abcd efgh jklm')).toBe('ABCD-EFGH-JKLM')
  })
  it('rejects anything else', () => {
    expect(fromQr('https://example.com')).toBeNull()
    expect(fromQr('rynuvo:transfer:SHORT')).toBeNull()
  })
})
