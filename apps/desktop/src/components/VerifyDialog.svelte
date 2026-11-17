<script lang="ts">
  import { onMount } from 'svelte'
  import { backdrop, pop } from '../lib/motion'
  import { api, errText } from '../lib/api'
  import { store } from '../lib/state.svelte'
  import type { Id, PersonCode } from '../lib/types'

  let { userId, onClose }: { userId: Id; onClose: () => void } = $props()

  let person = $state<PersonCode | null>(null)
  let loaded = $state(false)
  let error = $state<string | null>(null)
  let busy = $state(false)
  const groups = $derived(person ? person.code.split(/\s+/).filter(Boolean) : [])

  async function load() {
    try {
      person = await api.e2eePersonCode(userId)
    } catch (e) {
      error = errText(e)
    }
    loaded = true
  }
  onMount(() => void load())

  async function mark() {
    if (!person) return
    busy = true
    try {
      await api.e2eeVerify(userId, person.pubkey)
      void store.refreshSecurity()
      onClose()
    } catch {
      // Their key moved while the code was on screen: show the new one instead of the stale code.
      error = 'Their key changed. Compare the new code.'
      await load()
      void store.refreshSecurity()
    } finally {
      busy = false
    }
  }
</script>

<svelte:window onkeydown={(e) => e.key === 'Escape' && onClose()} />
<div class="overlay" transition:backdrop|global role="presentation" onclick={(e) => e.target === e.currentTarget && onClose()}>
  <div class="dlg" transition:pop|global role="dialog" aria-modal="true" aria-label="Verify {store.nick(userId)}">
    <h2>Verify {store.nick(userId)}</h2>
    {#if person}
      <div class="code" aria-label="Security code">{#each groups as g}<span>{g}</span>{/each}</div>
      <p class="hint">Compare this code with {store.nick(userId)} outside the call. If it matches, you are talking to them.</p>
    {:else if loaded}
      <p class="hint">No code available. Are you both still in the call?</p>
    {/if}
    {#if error}<p class="err">{error}</p>{/if}
    <div class="row">
      <button class="btn" onclick={onClose}>Close</button>
      <button class="btn primary" disabled={!person || busy} onclick={mark}>Mark as verified</button>
    </div>
  </div>
</div>

<style>
  .overlay { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.55); display: grid; place-items: center; z-index: 50; }
  .dlg { width: 380px; max-width: 90vw; background: var(--bg); border: 1px solid var(--line); border-radius: 20px; padding: 22px; box-shadow: 0 30px 80px rgba(0, 0, 0, 0.5); display: flex; flex-direction: column; gap: 12px; }
  h2 { margin: 0; font-size: 22px; font-weight: 750; font-stretch: 80%; font-variation-settings: 'opsz' 36; }
  .code { display: flex; flex-wrap: wrap; gap: 6px 14px; justify-content: center; padding: 14px; border-radius: 12px; background: var(--elev-2); font-family: ui-monospace, 'SF Mono', Menlo, monospace; font-size: 20px; letter-spacing: 1px; font-variant-numeric: tabular-nums; user-select: text; }
  .hint { margin: 0; font-size: 13px; color: var(--muted); line-height: 1.45; }
  .err { margin: 0; font-size: 13px; color: var(--danger); }
  .row { display: flex; justify-content: flex-end; gap: 8px; }
</style>
