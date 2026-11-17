/** On a phone, a popup inside a clipped or blurred panel moves to <body> so it can cover the
 *  screen as a sheet. (A `backdrop-filter` ancestor would trap even `position: fixed`.) */
export function phonePortal(node: HTMLElement) {
  if (!window.matchMedia('(max-width: 760px)').matches) return
  document.body.appendChild(node)
  return { destroy: () => node.remove() }
}
