<script lang="ts">
  import { backdrop, pop } from '../lib/motion'
  import { onDestroy, onMount } from 'svelte'
  import { api, errText } from '../lib/api'
  import { PRESETS, type Source } from '../lib/stream'
  import { store } from '../lib/state.svelte'

  let { onClose }: { onClose: () => void } = $props()

  let sources = $state<Source[] | null>(null)
  let error = $state<string | null>(null)
  let picked = $state<string | null>(null)
  let preset = $state(store.streamPreset)
  const screens = $derived((sources ?? []).filter((s) => s.kind === 'screen'))
  const windows = $derived((sources ?? []).filter((s) => s.kind === 'window'))

  // Small stills for every source, and a bigger, refreshing one of the picked source.
  let thumbs = $state<Record<string, string | null>>({})
  let preview = $state<{ id: string; url: string } | null>(null)
  let closed = false

  async function loadThumbs(list: Source[]) {
    const queue = list.filter((s) => !(s.id in thumbs))
    const worker = async () => {
      for (let s = queue.shift(); s && !closed; s = queue.shift()) {
        try {
          thumbs[s.id] = await api.streamThumbnail(s.id, 320, 180)
        } catch {
          thumbs[s.id] = null // minimized or gone: the card shows its name only
        }
      }
    }
    await Promise.all([worker(), worker(), worker()])
  }

  async function refreshPreview() {
    const id = picked
    if (!id || closed) return
    try {
      const url = await api.streamThumbnail(id, 960, 540)
      if (picked === id) preview = { id, url }
    } catch {
      if (picked === id) preview = null
    }
  }

  async function load() {
    error = null
    try {
      sources = await api.streamSources()
      if (!picked && sources.length) picked = sources[0].id
      void loadThumbs(sources)
    } catch (e) {
      error = errText(e)
      sources = []
    }
  }

  $effect(() => {
    if (picked) void refreshPreview()
  })

  let timer: ReturnType<typeof setInterval> | undefined
  onMount(() => {
    void load()
    timer = setInterval(() => {
      if (!document.hidden) void refreshPreview()
    }, 2000)
  })
  onDestroy(() => {
    closed = true
    clearInterval(timer)
  })

  async function start() {
    if (!picked) return
    if (await store.goLive(picked, preset)) onClose()
  }

  function key(e: KeyboardEvent) {
    if (e.key === 'Escape') onClose()
  }
</script>

<svelte:window onkeydown={key} />

<div class="overlay" transition:backdrop|global>
  <div class="box" transition:pop|global role="dialog" aria-modal="true" aria-label="Go Live">
    <h2>Go Live</h2>
    {#if error}
      <p class="err">{error}</p>
      <button class="btn" onclick={load}>Try again</button>
    {:else if sources === null}
      <p class="muted">Looking for screens and windows…</p>
    {:else}
      <div class="preview" aria-live="polite">
        {#if picked && preview?.id === picked}
          <img src={preview.url} alt="Preview of what you'll share" />
        {:else if picked && thumbs[picked]}
          <img src={thumbs[picked]} alt="Preview of what you'll share" />
        {:else}
          <span class="muted">{picked && thumbs[picked] === null ? 'No preview for this one' : 'Loading preview…'}</span>
        {/if}
      </div>
      <div class="list">
        {#each [{ title: 'Screens', items: screens }, { title: 'Windows', items: windows }] as group (group.title)}
          {#if group.items.length}
            <h3>{group.title}</h3>
            <div class="grid">
              {#each group.items as s (s.id)}
                <label class="src" class:on={picked === s.id} title={s.app ? `${s.name} — ${s.app}` : s.name}>
                  <input type="radio" name="src" value={s.id} bind:group={picked} />
                  <span class="thumb">
                    {#if thumbs[s.id]}<img src={thumbs[s.id]} alt="" />{/if}
                  </span>
                  <span class="name">{s.name}</span>
                  {#if s.app}<span class="app muted">{s.app}</span>{/if}
                </label>
              {/each}
            </div>
          {/if}
        {/each}
      </div>
      <p class="hint muted">A fullscreen game that shows up black as a window? Share its screen instead.</p>
      <label class="row">
        Show cursor
        <input type="checkbox" checked={store.streamCursor} onchange={(e) => void store.setStreamCursor(e.currentTarget.checked)} />
      </label>
      <label class="row">
        Quality
        <select bind:value={preset}>
          {#each PRESETS as p (p.id)}<option value={p.id}>{p.label}</option>{/each}
        </select>
      </label>
    {/if}
    <div class="actions">
      <button class="btn" onclick={onClose}>Cancel</button>
      <button class="btn primary" disabled={!picked || store.goingLive} onclick={start}>{store.goingLive ? 'Starting…' : 'Go Live'}</button>
    </div>
  </div>
</div>

<style>
  .overlay { position: fixed; inset: 0; background: rgba(8, 8, 12, 0.6); display: grid; place-items: center; z-index: 90; }
  .box { width: min(720px, 94vw); max-height: 90vh; display: flex; flex-direction: column; gap: 12px; background: var(--panel); border: 1px solid var(--line); border-radius: 12px; padding: 20px; }
  h2 { margin: 0; font-size: 18px; }
  h3 { margin: 10px 0 6px; font-size: 12px; text-transform: uppercase; letter-spacing: 0.04em; color: var(--faint); }
  .preview { aspect-ratio: 16 / 9; max-height: 30vh; border-radius: 8px; background: var(--elev); display: grid; place-items: center; overflow: hidden; flex: none; }
  .preview img { width: 100%; height: 100%; object-fit: contain; }
  .list { overflow-y: auto; min-height: 120px; }
  .grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(150px, 1fr)); gap: 8px; }
  .src { position: relative; display: flex; flex-direction: column; gap: 4px; padding: 6px; border-radius: 8px; cursor: pointer; border: 2px solid transparent; min-width: 0; }
  .src input { position: absolute; opacity: 0; pointer-events: none; }
  .src:hover { background: var(--elev); }
  .src.on { background: var(--elev-2); border-color: var(--accent); }
  .src:has(input:focus-visible) { outline: 2px solid var(--accent); outline-offset: 2px; }
  .thumb { aspect-ratio: 16 / 9; border-radius: 6px; background: var(--elev); overflow: hidden; display: block; }
  .thumb img { width: 100%; height: 100%; object-fit: cover; display: block; }
  .name { font-size: 13px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .app { font-size: 11px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .hint { font-size: 12px; margin: 0; }
  .row { display: flex; align-items: center; justify-content: space-between; gap: 12px; }
  .err { color: var(--danger); margin: 0; line-height: 1.45; }
  .actions { display: flex; justify-content: flex-end; gap: 8px; }
</style>
