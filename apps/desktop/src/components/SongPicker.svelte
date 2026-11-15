<script lang="ts">
  // Find a song and queue it in my voice channel, without typing !p.
  import Icon from './Icon.svelte'
  import { fly } from 'svelte/transition'
  import { api } from '../lib/api'
  import { store } from '../lib/state.svelte'
  import { clock, type SongResult } from '../lib/music'
  import type { Id } from '../lib/types'

  let { textChannelId, onClose }: { textChannelId: Id; onClose: () => void } = $props()

  const voice = $derived(store.myVoice.channelId)
  const voiceName = $derived(store.channels.find((c) => c.id === voice)?.name ?? 'voice')
  let q = $state('')
  let results = $state<SongResult[]>([])
  let busy = $state(false)
  let error = $state<string | null>(null)
  let pick = $state(0)
  let added = $state<string | null>(null)
  let input: HTMLInputElement | undefined = $state()

  $effect(() => input?.focus())

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
    busy = true
    const t = setTimeout(() => {
      api.musicSearch(term).then(
        (r) => { if (mine === seq) { results = r; pick = 0; busy = false } },
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
      setTimeout(onClose, 450)
    } catch (e) {
      error = String(e)
    }
  }

  function key(e: KeyboardEvent) {
    if (e.key === 'Escape') { e.preventDefault(); onClose() }
    else if (e.key === 'ArrowDown' && results.length) { e.preventDefault(); pick = (pick + 1) % results.length }
    else if (e.key === 'ArrowUp' && results.length) { e.preventDefault(); pick = (pick - 1 + results.length) % results.length }
    else if (e.key === 'Enter' && results[pick]) { e.preventDefault(); void play(results[pick]) }
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="scrim" onmousedown={onClose}></div>
<div class="picker" role="dialog" aria-label="Play a song" transition:fly={{ y: 8, duration: 160 }}>
  <div class="search">
    <Icon name="music" size={16} />
    <input bind:this={input} bind:value={q} onkeydown={key} placeholder="Search a song or artist" aria-label="Search a song or artist" />
    {#if busy}<span class="spin" aria-label="Searching"></span>{/if}
  </div>
  {#if voice === null}
    <p class="hint">Join a voice channel first, then pick a song to play there.</p>
  {/if}
  {#if error}<p class="hint err">{error}</p>{/if}
  {#if results.length}
    <ul role="listbox" aria-label="Songs">
      {#each results as s, i (s.videoId)}
        <li>
          <button
            role="option"
            aria-selected={i === pick}
            class:on={i === pick}
            disabled={voice === null}
            onmouseenter={() => (pick = i)}
            onclick={() => play(s)}
          >
            <span class="art" style="background-image:url('{s.artUrl}')"></span>
            <span class="txt"><b>{s.title}</b><small>{s.artist}{s.album ? ` · ${s.album}` : ''}</small></span>
            <span class="len">{clock(s.durationMs)}</span>
            <span class="go" class:done={added === s.videoId}><Icon name={added === s.videoId ? 'check' : 'play'} size={14} /></span>
          </button>
        </li>
      {/each}
    </ul>
  {:else if q.trim() && !busy && !error}
    <p class="hint">Nothing found for “{q.trim()}”</p>
  {/if}
  {#if voice !== null}
    <p class="foot">Plays in <b>{voiceName}</b> · ↑↓ to choose, Enter to play</p>
  {/if}
</div>

<style>
  .scrim { position: fixed; inset: 0; z-index: 4; }
  .picker {
    position: absolute; left: 0; right: 0; bottom: calc(100% + 6px); z-index: 5; max-width: 520px; margin-left: auto;
    background: var(--panel); border: 1px solid var(--line); border-radius: 12px; padding: 8px; box-shadow: 0 12px 32px rgba(0, 0, 0, 0.4);
  }
  .search { display: flex; align-items: center; gap: 8px; padding: 6px 8px 8px; color: var(--muted); border-bottom: 1px solid var(--line); }
  .search input { flex: 1; background: transparent; border: 0; outline: none; color: var(--text); font-size: 14px; }
  .spin { width: 14px; height: 14px; border-radius: 50%; border: 2px solid var(--line); border-top-color: var(--accent); animation: spin 700ms linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  ul { list-style: none; margin: 6px 0 0; padding: 0; max-height: 320px; overflow-y: auto; }
  li button { width: 100%; display: flex; align-items: center; gap: 10px; padding: 6px 8px; border-radius: 8px; text-align: left; color: var(--text); }
  li button.on:not(:disabled) { background: var(--elev); }
  li button:disabled { opacity: 0.55; cursor: not-allowed; }
  .art { flex: none; width: 40px; height: 40px; border-radius: 6px; background-size: cover; background-position: center; background-color: var(--elev-2); }
  .txt { flex: 1; min-width: 0; display: flex; flex-direction: column; }
  .txt b, .txt small { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .txt b { font-size: 13.5px; }
  .txt small { color: var(--muted); font-size: 12px; }
  .len { font-size: 12px; color: var(--muted); font-variant-numeric: tabular-nums; }
  .go { width: 28px; height: 28px; border-radius: 50%; display: grid; place-items: center; background: var(--elev-2); color: var(--text); opacity: 0; transition: opacity 120ms, background 120ms; }
  li button.on .go, .go.done { opacity: 1; background: var(--accent); color: #fff; }
  .hint { margin: 8px; font-size: 13px; color: var(--muted); }
  .hint.err { color: var(--danger); }
  .foot { margin: 6px 8px 2px; font-size: 11.5px; color: var(--faint); }
  .foot b { color: var(--muted); font-weight: 600; }
  @media (prefers-reduced-motion: reduce) { .spin { animation: none; } }
</style>
