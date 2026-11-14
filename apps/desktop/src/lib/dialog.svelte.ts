/** In-app replacement for window.confirm/alert, which the Tauri webview does not show. */
export interface Prompt {
  message: string
  ok: string
  cancel: string | null
  danger: boolean
  resolve: (v: boolean) => void
}

class Dialog {
  current = $state<Prompt | null>(null)

  private open(message: string, ok: string, cancel: string | null, danger: boolean): Promise<boolean> {
    this.current?.resolve(false)
    return new Promise((resolve) => (this.current = { message, ok, cancel, danger, resolve }))
  }

  ask(message: string, ok = 'OK', danger = true): Promise<boolean> {
    return this.open(message, ok, 'Cancel', danger)
  }

  notice(message: string): Promise<boolean> {
    return this.open(message, 'OK', null, false)
  }

  answer(v: boolean) {
    const p = this.current
    this.current = null
    p?.resolve(v)
  }
}

export const dialog = new Dialog()
