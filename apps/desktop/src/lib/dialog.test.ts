import { describe, expect, it } from 'vitest'
import { dialog } from './dialog.svelte'

describe('dialog', () => {
  it('resolves ask() with the answer and clears the prompt', async () => {
    const p = dialog.ask('Delete?', 'Delete')
    expect(dialog.current?.message).toBe('Delete?')
    dialog.answer(true)
    expect(await p).toBe(true)
    expect(dialog.current).toBeNull()
  })
  it('a second ask cancels the first', async () => {
    const first = dialog.ask('A')
    const second = dialog.ask('B')
    expect(await first).toBe(false)
    dialog.answer(true)
    expect(await second).toBe(true)
  })
  it('notice() shows a message with only an OK button', async () => {
    const p = dialog.notice('Failed')
    expect(dialog.current?.cancel).toBeNull()
    dialog.answer(true)
    await p
  })
})
