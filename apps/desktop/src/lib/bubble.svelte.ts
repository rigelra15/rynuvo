// Android: the call bubble that floats over other apps while Rynuvo is in the background.
// It needs Android's "display over other apps", which only its Settings page can turn on.

type Status = 'on' | 'off' | 'permission'
interface BubbleBridge {
  bubbleStatus(): string
  bubbleSet(on: boolean): void
  bubbleAllow(): void
}
const bridge = () => {
  const b = (window as unknown as { RynuvoAndroid?: Partial<BubbleBridge> }).RynuvoAndroid
  return b?.bubbleStatus && b.bubbleSet && b.bubbleAllow ? (b as BubbleBridge) : null
}

const HINTED = 'rynuvo.bubbleHinted'

class Bubble {
  /** null: no bubble on this platform (or an app too old for it). */
  status = $state<Status | null>(null)
  /** The prompt in the room was answered or put away. */
  hinted = $state(false)

  init() {
    this.refresh()
    try {
      this.hinted = localStorage.getItem(HINTED) === '1'
    } catch {
      // Private storage off: the prompt may show again, which is harmless.
    }
    // Back from Android's settings page: it may have been switched on.
    document.addEventListener('visibilitychange', () => {
      if (document.visibilityState === 'visible') this.refresh()
    })
  }

  refresh() {
    const b = bridge()
    this.status = b ? (b.bubbleStatus() as Status) : null
  }

  /** On: asks Android for the permission first if it's missing. */
  set(on: boolean) {
    const b = bridge()
    if (!b) return
    b.bubbleSet(on)
    if (on && b.bubbleStatus() === 'permission') b.bubbleAllow()
    this.refresh()
  }

  hide() {
    this.hinted = true
    try {
      localStorage.setItem(HINTED, '1')
    } catch {
      // See init.
    }
  }
}

export const bubble = new Bubble()
