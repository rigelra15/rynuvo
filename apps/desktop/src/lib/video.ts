/** Music videos: where each one streams from (asked once), and keeping a silent video in step with
 *  the song everyone hears. */
type Entry = { videoId: string; video?: boolean }

export function videoUrls(get: (videoId: string) => Promise<string>) {
  const known = new Map<string, Promise<string>>()
  function url(id: string): Promise<string> {
    let p = known.get(id)
    if (!p) {
      p = get(id)
      known.set(id, p)
      // A failed ask is forgotten, so the next look tries again.
      p.catch(() => known.delete(id))
    }
    return p
  }
  return {
    get: url,
    /** Ask early for the playing video and the next one, so they show the moment they're needed. */
    prefetch(s: { now?: Entry | null; queue: Entry[] }) {
      for (const e of [s.now, s.queue[0]]) if (e?.video) url(e.videoId).catch(() => {})
    },
  }
}

/** How to bring a video that is `drift` seconds off (ahead > 0) back in step. */
export function catchUp(drift: number): { seek: boolean; rate: number } {
  if (Math.abs(drift) > 0.6) return { seek: true, rate: 1 }
  if (Math.abs(drift) > 0.06) return { seek: false, rate: drift > 0 ? 0.95 : 1.05 }
  return { seek: false, rate: 1 }
}

/** Steer `v` to `posMs` (the song's position), playing or paused like the song. */
export function follow(v: HTMLVideoElement, posMs: number, paused: boolean) {
  if (paused) { if (!v.paused) v.pause() }
  else if (v.paused) void v.play().catch(() => {})
  if (v.seeking) return
  const want = posMs / 1000
  const c = catchUp(v.currentTime - want)
  if (c.seek) v.currentTime = want
  else v.playbackRate = c.rate
}
