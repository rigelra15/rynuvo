// Lyrics of the songs playing, fetched once per song and shared by the player and the sidebar card.
import { SvelteMap } from 'svelte/reactivity'
import { api } from './api'
import type { Annotation, Lyrics, MusicEntry } from './music'

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

const notes = new SvelteMap<string, Annotation[] | null>()
const noted = new Set<string>()

/** Translation and romanization of `lines` (asked once per `key`): `undefined` while loading. */
export function annotationsOf(key: string, lines: string[]): Annotation[] | null | undefined {
  if (!noted.has(key)) {
    noted.add(key)
    void api.musicAnnotate(lines).then(
      (a) => notes.set(key, a),
      () => notes.set(key, null),
    )
  }
  return notes.get(key)
}

/** Whether to show romanization and English translation under lyric lines (this app only). */
export const lyricExtras = (() => {
  const read = (k: string) => {
    try {
      return localStorage.getItem(k) !== '0'
    } catch {
      return true
    }
  }
  let roman = $state(read('lyrics.roman'))
  let translate = $state(read('lyrics.translate'))
  const save = (k: string, v: boolean) => {
    try {
      localStorage.setItem(k, v ? '1' : '0')
    } catch {
      // not remembered; fine
    }
  }
  return {
    get roman() { return roman },
    set roman(v: boolean) { roman = v; save('lyrics.roman', v) },
    get translate() { return translate },
    set translate(v: boolean) { translate = v; save('lyrics.translate', v) },
  }
})()

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
