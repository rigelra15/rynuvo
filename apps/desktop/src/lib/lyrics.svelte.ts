// Lyrics of the songs playing, fetched once per song and shared by the player and the sidebar card.
import { SvelteMap } from 'svelte/reactivity'
import { api } from './api'
import type { Lyrics, MusicEntry } from './music'

const cache = new SvelteMap<string, Lyrics | null>()
const asked = new Set<string>()

/** The song's lyrics from `source`: `undefined` while loading, `null` when there are none. */
export function lyricsOf(e: MusicEntry, source = 'auto'): Lyrics | null | undefined {
  const key = `${e.videoId}|${source}`
  if (!asked.has(key)) {
    asked.add(key)
    void api.musicLyrics(e.videoId, e.title, e.artist, e.durationMs, source).then(
      (l) => cache.set(key, l),
      () => cache.set(key, null),
    )
  }
  return cache.get(key)
}

/** A clock (ms) that follows the screen's frame rate while `on`, for motion that has to be smooth. */
export function frameClock(on: () => boolean) {
  let t = $state(Date.now())
  $effect(() => {
    if (!on()) return
    let id = requestAnimationFrame(function tick() {
      t = Date.now()
      id = requestAnimationFrame(tick)
    })
    return () => cancelAnimationFrame(id)
  })
  return { get now() { return t } }
}
