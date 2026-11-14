<script lang="ts">
  import { api, errText } from '../lib/api'
  import { store } from '../lib/state.svelte'

  let code = $state('')
  let nickname = $state('')
  let transfer = $state(false)
  let busy = $state(false)
  let error = $state<string | null>(null)

  async function submit(e: SubmitEvent) {
    e.preventDefault()
    busy = true
    error = null
    try {
      const me = transfer ? await api.claimTransfer(code) : await api.join(code, nickname)
      store.signedIn(me)
    } catch (err) {
      error = errText(err)
    } finally {
      busy = false
    }
  }
</script>

<div class="wrap">
  <form class="card" onsubmit={submit}>
    <div class="logo">◆</div>
    <h1>Sign in to Rynuvo</h1>
    {#if store.notice}<p class="notice">{store.notice}</p>{/if}

    <label>
      {transfer ? 'Transfer code' : 'Invite code'}
      <input bind:value={code} placeholder="XXXX-XXXX-XXXX" autocomplete="off" spellcheck="false" required />
    </label>
    {#if !transfer}
      <label>Nickname<input bind:value={nickname} maxlength="32" required /></label>
    {/if}

    {#if error}<p class="err">{error}</p>{/if}
    <button class="btn primary" disabled={busy}>{busy ? 'Connecting…' : 'Sign in'}</button>
    <button type="button" class="link" onclick={() => ((transfer = !transfer), (error = null))}>
      {transfer ? 'Use an invite code' : 'Have a transfer code from another device?'}
    </button>
  </form>
</div>

<style>
  .wrap { height: 100%; display: grid; place-items: center; background: radial-gradient(circle at 30% 20%, #2b2350, var(--bg) 60%); }
  .card { width: 360px; background: var(--panel); border: 1px solid var(--line); border-radius: 14px; padding: 28px; display: flex; flex-direction: column; gap: 14px; }
  .logo { color: var(--accent); font-size: 28px; }
  h1 { margin: 0 0 4px; font-size: 20px; }
  label { display: flex; flex-direction: column; gap: 6px; font-size: 12px; font-weight: 600; color: var(--muted); text-transform: uppercase; letter-spacing: 0.04em; }
  label input { text-transform: none; letter-spacing: normal; font-weight: 400; font-size: 14px; }
  .err { color: var(--danger); margin: 0; font-size: 13px; }
  .notice { margin: 0; padding: 8px 10px; border-radius: 8px; background: rgba(239, 68, 68, 0.12); color: #fca5a5; font-size: 13px; }
  .link { color: var(--accent); font-size: 13px; }
</style>
