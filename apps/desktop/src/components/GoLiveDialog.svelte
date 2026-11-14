<script lang="ts">
  import { backdrop, pop } from '../lib/motion'
  import { onMount } from 'svelte'
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

  async function load() {
    error = null
    try {
      sources = await api.streamSources()
      if (!picked && sources.length) picked = sources[0].id
    } catch (e) {
      error = errText(e)
      sources = []
    }
  }

  onMount(() => void load())

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
      <div class="list">
        {#if screens.length}<h3>Screens</h3>{/if}
        {#each screens as s (s.id)}
          <label class="src" class:on={picked === s.id}>
            <input type="radio" name="src" value={s.id} bind:group={picked} />
            <span class="name">{s.name}</span>
          </label>
        {/each}
        {#if windows.length}<h3>Windows</h3>{/if}
        {#each windows as s (s.id)}
          <label class="src" class:on={picked === s.id}>
            <input type="radio" name="src" value={s.id} bind:group={picked} />
            <span class="name">{s.name}</span>
            {#if s.app}<span class="app muted">{s.app}</span>{/if}
          </label>
        {/each}
      </div>
      <p class="hint muted">A fullscreen game that shows up black as a window? Share its screen instead.</p>
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
  .box { width: min(520px, 92vw); max-height: 86vh; display: flex; flex-direction: column; gap: 12px; background: var(--panel); border: 1px solid var(--line); border-radius: 12px; padding: 20px; }
  h2 { margin: 0; font-size: 18px; }
  h3 { margin: 10px 0 4px; font-size: 12px; text-transform: uppercase; letter-spacing: 0.04em; color: var(--faint); }
  .list { overflow-y: auto; min-height: 120px; display: flex; flex-direction: column; gap: 4px; }
  .src { display: flex; align-items: center; gap: 10px; padding: 8px 10px; border-radius: 8px; cursor: pointer; }
  .src:hover { background: var(--elev); }
  .src.on { background: var(--elev-2); }
  .name { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .app { font-size: 12px; }
  .hint { font-size: 12px; margin: 0; }
  .row { display: flex; align-items: center; justify-content: space-between; gap: 12px; }
  .err { color: var(--danger); margin: 0; line-height: 1.45; }
  .actions { display: flex; justify-content: flex-end; gap: 8px; }
</style>
