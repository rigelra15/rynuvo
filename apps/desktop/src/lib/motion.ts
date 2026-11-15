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

/** A context menu: grows from its top-left corner (where it was opened). */
export function menu(_: Element, _p = {}, { direction }: { direction: 'in' | 'out' | 'both' } = { direction: 'in' }): TransitionConfig {
  return {
    duration: reduced() ? 0 : direction === 'out' ? 100 : 140,
    easing: cubicOut,
    css: (t) => `opacity:${t};transform:scale(${0.94 + 0.06 * t});transform-origin:top left`,
  }
}

/** The music panel beside the voice room: its slot widens (or narrows) while the panel inside keeps
 *  its size, riding the slot's left edge, so it slides in from (and out past) the right edge whole. */
export function sidePanel(
  node: HTMLElement,
  { duration = 520 } = {},
  { direction }: { direction: 'in' | 'out' | 'both' } = { direction: 'in' },
): TransitionConfig {
  const w = node.offsetWidth
  const inner = node.firstElementChild as HTMLElement | null
  if (inner) inner.style.minWidth = `${w}px`
  return {
    duration: reduced() ? 0 : duration,
    easing: cubicInOut,
    css: (t) => `width:${(t * w).toFixed(2)}px;margin-left:${((t - 1) * 16).toFixed(2)}px;opacity:${Math.min(1, t * 1.6).toFixed(3)}`,
    // Once fully open, the panel follows the window's size again (closing keeps it fixed to the end).
    tick: (t) => { if (direction === 'in' && t === 1 && inner) inner.style.minWidth = '' },
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
