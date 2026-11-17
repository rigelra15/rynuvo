/** The time, for "3m ago" style labels: one shared tick a minute, not a timer per label. */
export const clock = $state({ now: Date.now() })
setInterval(() => (clock.now = Date.now()), 30_000)
