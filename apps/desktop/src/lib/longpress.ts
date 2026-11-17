/** On iOS a long press opens the same menu a right click does: after holding still for `HOLD_MS`, a
 *  `contextmenu` event goes to the pressed element, and the tap that ends the press is swallowed so it
 *  doesn't also click. Android's WebView does this on its own. */
const HOLD_MS = 480
const SLOP = 10

export function installLongPress(doc: Document = document) {
  let timer: ReturnType<typeof setTimeout> | null = null
  let start: { x: number; y: number } | null = null
  let fired = false
  const cancel = () => {
    if (timer) clearTimeout(timer)
    timer = null
    start = null
  }
  doc.addEventListener('touchstart', (e) => {
    cancel()
    fired = false
    if (e.touches.length !== 1) return
    const t = e.touches[0]
    const target = t.target as Element
    start = { x: t.clientX, y: t.clientY }
    timer = setTimeout(() => {
      timer = null
      fired = true
      target.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: start!.x, clientY: start!.y }))
    }, HOLD_MS)
  }, { passive: true })
  doc.addEventListener('touchmove', (e) => {
    const t = e.touches[0]
    if (start && t && Math.hypot(t.clientX - start.x, t.clientY - start.y) > SLOP) cancel()
  }, { passive: true })
  doc.addEventListener('touchend', (e) => {
    cancel()
    if (fired) e.preventDefault() // no click after the menu opened
  })
  doc.addEventListener('touchcancel', cancel)
}
