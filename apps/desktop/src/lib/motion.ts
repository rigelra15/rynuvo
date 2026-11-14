/** Enter/exit transitions for overlays. Opacity + transform only; instant under reduced motion. */
import { cubicOut, cubicIn } from 'svelte/easing'
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
