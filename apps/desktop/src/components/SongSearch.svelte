<script lang="ts" module>
  import type { SongResult as Song } from '../lib/music'
  // The music panel's search outlives the panel itself: when a song ends and the panel switches
  // between the player and the idle view, what I was typing and its results stay.
  const kept = $state({ text: '', results: [] as Song[], focused: false, caret: 0 })
  const recent = new Map<string, Song[]>()
</script>

<script lang="ts">
  // Search YouTube Music and queue a song in my voice channel. Used in the chat composer's picker
  // and in the empty music panel.
  import Icon from './Icon.svelte'
  import { api } from '../lib/api'
  import { store } from '../lib/state.svelte'
  import { clock, type SongResult } from '../lib/music'
  import type { Id } from '../lib/types'

  let {
    textChannelId = null,
    onPlayed,
    onEscape,
    glass = false,
    autofocus = true,
    query = null,
    keep = false,
  }: {
    textChannelId?: Id | null
    onPlayed?: () => void
    onEscape?: () => void
    glass?: boolean
    autofocus?: boolean
    /** Search this text instead of an own search box (the chat's "!p …" being typed). */
    query?: string | null
    /** Share the typed text and results with the other music-panel search (see `kept`). */
    keep?: boolean
  } = $props()

  const voice = $derived(store.myVoice.channelId)
  const voiceName = $derived(store.channels.find((c) => c.id === voice)?.name ?? 'voice')
  // svelte-ignore state_referenced_locally
  let own = $state(keep ? kept.text : '')
  const q = $derived(query ?? own)
  // svelte-ignore state_referenced_locally
  let results = $state<SongResult[]>(keep ? kept.results : [])
  let busy = $state(false)
  let error = $state<string | null>(null)
  let pick = $state(0)
  let added = $state<string | null>(null)
  let input: HTMLInputElement | undefined = $state()

  $effect(() => {
    if (autofocus) input?.focus()
  })

  $effect(() => {
    if (!keep) return
    kept.text = own
    kept.results = results
  })
  // Typing when the panel swapped under me: keep the caret where it was.
  $effect(() => {
    if (keep && kept.focused && input) {
      input.focus()
      input.setSelectionRange(kept.caret, kept.caret)
    }
  })
  function remember() {
    if (keep && input) {
      kept.focused = document.activeElement === input
      kept.caret = input.selectionStart ?? own.length
    }
  }

  // Search as you type, after a short pause.
  let seq = 0
  $effect(() => {
    const term = q.trim()
    error = null
    if (!term) {
      results = []
      busy = false
      return
    }
    const mine = ++seq
    const hit = recent.get(term)
    if (hit) {
      results = hit
      busy = false
      return
    }
    busy = true
    const t = setTimeout(() => {
      api.musicSearch(term).then(
        // Songs first, then music videos: the keys walk them in the order they are shown.
        (r) => {
          const sorted = [...r.filter((x) => !x.video), ...r.filter((x) => x.video)]
          recent.set(term, sorted)
          if (recent.size > 30) recent.delete(recent.keys().next().value!)
          if (mine === seq) { results = sorted; pick = 0; busy = false }
        },
        (e) => { if (mine === seq) { error = String(e); results = []; busy = false } },
      )
    }, 350)
    return () => clearTimeout(t)
  })

  async function play(s: SongResult) {
    if (voice === null) return
    try {
      await api.musicCmd({ op: 'add', entry: s }, textChannelId)
      added = s.videoId
      setTimeout(() => {
        added = null
        own = ''
        onPlayed?.()
      }, 450)
    } catch (e) {
      error = String(e)
    }
  }

  /** Arrow keys, Enter and Escape for the results; true when the key was used. Called by the chat
   *  box too, since there the caret stays in the message. */
  export function key(e: KeyboardEvent): boolean {
    if (e.key === 'Escape') onEscape?.()
    else if (e.key === 'ArrowDown' && results.length) pick = (pick + 1) % results.length
    else if (e.key === 'ArrowUp' && results.length) pick = (pick - 1 + results.length) % results.length
    else if ((e.key === 'Enter' || e.key === 'Tab') && results[pick] && !e.shiftKey && voice !== null) void play(results[pick])
    else return false
    e.preventDefault()
    return true
  }
</script>

<div class="ss" class:glass>
  {#if query === null}
    <div class="search">
      <Icon name="music" size={16} />
      <input bind:this={input} bind:value={own} onkeydown={key} onkeyup={remember} onfocus={remember} onblur={() => { if (keep && input?.isConnected) kept.focused = false }} oninput={remember} placeholder="Search a song or artist" aria-label="Search a song or artist" />
      {#if busy}<span class="spin" aria-label="Searching"></span>{/if}
    </div>
  {:else}
    <div class="typed"><Icon name="music" size={14} /> {q.trim() ? `Songs for “${q.trim()}”` : 'Type a song or artist'}{#if busy}<span class="spin" aria-label="Searching"></span>{/if}</div>
  {/if}
  {#if voice === null}
    <p class="hint">Join a voice channel first, then pick a song to play there.</p>
  {/if}
  {#if error}<p class="hint err">{error}</p>{/if}
  {#if results.length}
    {@const songs = results.filter((x) => !x.video).length}
    <div role="listbox" aria-label="Songs and videos" class="lists">
      {#each [{ name: 'Songs', from: 0, to: songs }, { name: 'Videos', from: songs, to: results.length }] as sec (sec.name)}
        {#if sec.to > sec.from}
          <div class="sec" role="group" aria-label={sec.name}>
            <h4>{sec.name}</h4>
            <ul>
              {#each results.slice(sec.from, sec.to) as s, k (s.videoId)}
                {@const i = sec.from + k}
                <li>
                  <button
                    role="option"
                    aria-selected={i === pick}
                    class:on={i === pick}
                    disabled={voice === null}
                    onmouseenter={() => (pick = i)}
                    onclick={() => play(s)}
                  >
                    <span class="art" class:wide={s.video} style="background-image:url('{s.artUrl}')"></span>
                    <span class="txt"><b>{s.title}</b><small>{s.artist}{s.album ? ` · ${s.album}` : ''}</small></span>
                    <span class="len">{clock(s.durationMs)}</span>
                    <span class="go" class:done={added === s.videoId}><Icon name={added === s.videoId ? 'check' : 'play'} size={14} /></span>
                  </button>
                </li>
              {/each}
            </ul>
          </div>
        {/if}
      {/each}
    </div>
  {:else if q.trim() && !busy && !error}
    <p class="hint">Nothing found for “{q.trim()}”</p>
  {/if}
  {#if voice !== null && !glass}
    <p class="foot">Plays in <b>{voiceName}</b> · ↑↓ to choose, Enter to play{query !== null ? ', Esc to just send the text' : ''}</p>
  {/if}
</div>

<style>
  .ss { --fg: var(--text); --sub: var(--muted); --hover: var(--elev); --chip: var(--elev-2); --rule: var(--line); display: flex; flex-direction: column; min-height: 0; }
  .ss.glass { --fg: #fff; --sub: rgba(255, 255, 255, 0.65); --hover: rgba(255, 255, 255, 0.1); --chip: rgba(255, 255, 255, 0.12); --rule: rgba(255, 255, 255, 0.1); }
  .search { display: flex; align-items: center; gap: 8px; padding: 6px 8px 8px; color: var(--sub); border-bottom: 1px solid var(--rule); }
  .glass .search { border: 1px solid var(--rule); border-radius: 10px; padding: 9px 12px; background: rgba(0, 0, 0, 0.2); }
  .typed { display: flex; align-items: center; gap: 6px; padding: 4px 8px 6px; font-size: 12px; font-weight: 600; color: var(--sub); }
  .typed .spin { margin-left: auto; }
  .search input { flex: 1; background: transparent; border: 0; outline: none; color: var(--fg); font-size: 14px; }
  .search input::placeholder { color: var(--sub); }
  .spin { width: 14px; height: 14px; border-radius: 50%; border: 2px solid var(--rule); border-top-color: var(--accent); animation: spin 700ms linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  .lists { margin-top: 6px; max-height: 320px; overflow-y: auto; }
  .glass .lists { max-height: none; flex: 1; min-height: 0; }
  ul { list-style: none; margin: 0; padding: 0; }
  .sec + .sec { margin-top: 8px; }
  h4 { margin: 0; padding: 4px 8px 4px; font-size: 11px; font-weight: 700; letter-spacing: 0.06em; text-transform: uppercase; color: var(--sub); }
  li button { width: 100%; display: flex; align-items: center; gap: 10px; padding: 6px 8px; border-radius: 8px; text-align: left; color: var(--fg); }
  li button.on:not(:disabled) { background: var(--hover); }
  li button:disabled { opacity: 0.55; cursor: not-allowed; }
  .art { flex: none; width: 40px; height: 40px; border-radius: 6px; background-size: cover; background-position: center; background-color: var(--chip); }
  /* A video's thumbnail keeps its 16:9. */
  .art.wide { width: 71px; }
  .txt { flex: 1; min-width: 0; display: flex; flex-direction: column; }
  .txt b, .txt small { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .txt b { font-size: 13.5px; }
  .txt small { color: var(--sub); font-size: 12px; }
  .len { font-size: 12px; color: var(--sub); font-variant-numeric: tabular-nums; }
  .go { width: 28px; height: 28px; border-radius: 50%; display: grid; place-items: center; background: var(--chip); color: var(--fg); opacity: 0; transition: opacity 120ms, background 120ms; }
  li button.on .go, .go.done { opacity: 1; background: var(--accent); color: var(--on-accent); }
  .hint { margin: 8px; font-size: 13px; color: var(--sub); }
  .hint.err { color: var(--danger); }
  .foot { margin: 6px 8px 2px; font-size: 11.5px; color: var(--faint); }
  .foot b { color: var(--muted); font-weight: 600; }
  @media (prefers-reduced-motion: reduce) { .spin { animation: none; } }
</style>
