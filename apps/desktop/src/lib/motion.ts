/** Enter/exit transitions for overlays. Opacity + transform only; instant under reduced motion. */
import { cubicOut, cubicIn, cubicInOut, quintOut } from 'svelte/easing'
import type { TransitionConfig } from 'svelte/transition'

const reduced = () => typeof matchMedia === 'function' && matchMedia('(prefers-reduced-motion: reduce)').matches

/** Dimmed background behind a dialog. */
export function backdrop(_: Element, { duration = 180 } = {}): TransitionConfig {
  return { duration: reduced() ? 0 : duration, css: (t) => `opacity:${t}` }
}

/** A dialog card: rises and grows in, sinks and shrinks out (shorter on the way out). */
export function pop(_: Element, _p = {}, { direction }: { direction: 'in' | 'out' | 'both' } = { direction: 'in' }): TransitionConfig {
  const out = direction === 'out'
  return {
    duration: reduced() ? 0 : out ? 140 : 220,
    easing: out ? cubicIn : cubicOut,
    css: (t, u) => `opacity:${t};transform:translateY(${u * 10}px) scale(${0.96 + 0.04 * t})`,
  }
}

/** A full-screen panel: on a phone it slides up from the bottom edge (and back down); elsewhere it pops. */
export function sheet(node: Element, p = {}, opts: { direction: 'in' | 'out' | 'both' } = { direction: 'in' }): TransitionConfig {
  if (typeof matchMedia !== 'function' || !matchMedia('(max-width: 760px)').matches) return pop(node, p, opts)
  const out = opts.direction === 'out'
  return {
    duration: reduced() ? 0 : out ? 220 : 380,
    easing: out ? cubicIn : quintOut,
    css: (_t, u) => `transform:translateY(${(u * 100).toFixed(2)}%)`,
  }
}

/** A context menu: grows from its top-left corner (where it was opened). */
export function menu(_: Element, _p = {}, { direction }: { direction: 'in' | 'out' | 'both' } = { direction: 'in' }): TransitionConfig {
  return {
    duration: reduced() ? 0 : direction === 'out' ? 100 : 140,
    easing: cubicOut,
    css: (t) => `opacity:${t};transform:scale(${0.94 + 0.06 * t});transform-origin:top left`,
  }
}

const opened = new WeakSet<Node>()

/** On the music panel's slot (with `transition:sidePanel`): once it's on screen, the next run of the
 *  transition is a close. Covers a panel that was already open when the page loaded (no intro ran). */
export function panelShown(node: HTMLElement) {
  requestAnimationFrame(() => opened.add(node))
}

/** The music panel beside the voice room: its slot widens (or narrows) while the panel inside keeps
 *  its size, riding the slot's left edge, so it slides in from (and out past) the right edge whole. */
export function sidePanel(
  node: HTMLElement,
  { duration = 520 } = {},
  _opts: { direction: 'in' | 'out' | 'both' } = { direction: 'both' },
): TransitionConfig {
  const inner = node.firstElementChild as HTMLElement | null
  // `transition:` runs both ways and Svelte only says "both", so: a node seen for the first time is
  // opening; after that it's closing (or turning back, which the direction of `t` tells).
  const opening = !opened.has(node)
  opened.add(node)
  // Stacked under the people (phones): it rises from below while its slot opens, the room above
  // easing up to make space, instead of squeezing sideways. Driven frame by frame: the panel's
  // resting height is measured again every frame (its content can still grow while it opens), so
  // it ends exactly where it settles, with no jump on the last frame.
  if (node.parentElement && getComputedStyle(node.parentElement).flexDirection === 'column') {
    let from = node.offsetHeight
    let prev: number | null = null
    const set = (h: number, t: number, u: number) => {
      // An explicit height becomes its flex basis, which flex would then shrink (492 for 523 with a
      // song playing) and jump back from at the end: no shrinking while it animates.
      node.style.flexShrink = '0'
      node.style.height = `${h.toFixed(2)}px`
      node.style.marginTop = `${(-u * 12).toFixed(2)}px`
      node.style.opacity = Math.min(1, t * 1.4).toFixed(3)
      node.style.transform = `translateY(${(u * 40).toFixed(2)}px)`
    }
    const clear = () => {
      for (const k of ['height', 'marginTop', 'opacity', 'transform', 'overflow', 'flexShrink'] as const) node.style[k] = ''
      if (inner) inner.style.minHeight = ''
    }
    /** The content keeps its resting size and slides; only the slot round it opens or closes. */
    const hold = (h: number) => {
      node.style.overflow = 'hidden'
      if (inner) inner.style.minHeight = `${h}px`
    }
    // Closed before the first frame is drawn (else it shows full height for a frame, then collapses).
    if (opening) {
      hold(from)
      set(0, 0, 1)
    }
    return {
      duration: reduced() ? 0 : opening ? 480 : 380,
      easing: opening ? quintOut : cubicInOut,
      tick: (t, u) => {
        const closing = prev === null ? !opening : t < prev
        prev = t
        if (closing) {
          if (node.style.overflow !== 'hidden') {
            from = node.offsetHeight
            hold(from)
          }
          return set(t * from, t, u)
        }
        if (t >= 1) return clear()
        // Measured again every frame, as it will be at rest (its content can still grow while it
        // opens, and clipping changes how flex sizes it), so it ends exactly where it settles.
        node.style.height = ''
        node.style.overflow = ''
        node.style.flexShrink = ''
        if (inner) inner.style.minHeight = ''
        from = node.offsetHeight
        hold(from)
        set(t * from, t, u)
      },
    }
  }
  const w = node.offsetWidth
  if (inner) inner.style.minWidth = `${w}px`
  return {
    duration: reduced() ? 0 : opening ? duration : 420,
    easing: cubicInOut,
    css: (t) => `width:${(t * w).toFixed(2)}px;margin-left:${((t - 1) * 16).toFixed(2)}px;opacity:${Math.min(1, t * 1.6).toFixed(3)}`,
    // Once fully open, the panel follows the window's size again (closing keeps it fixed to the end).
    tick: (t) => { if (opening && t >= 1 && inner) inner.style.minWidth = '' },
  }
}

/** The call controls: rise from below and open out from the middle into a pill. */
export function dockIn(_: Element, { duration = 640 } = {}): TransitionConfig {
  return {
    duration: reduced() ? 0 : duration,
    easing: quintOut,
    css: (t, u) => `opacity:${Math.min(1, t * 2).toFixed(3)};transform:translateY(${(u * 26).toFixed(2)}px);clip-path:inset(0 ${(u * 46).toFixed(2)}% round 999px)`,
  }
}
