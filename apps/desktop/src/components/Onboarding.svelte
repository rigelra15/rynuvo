<script lang="ts">
  import Icon from './Icon.svelte'
  import QrScanner from './QrScanner.svelte'
  import { api, errText } from '../lib/api'
  import { store } from '../lib/state.svelte'
  import { formatCode, fromQr } from '../lib/code'
  import { isMobile } from '../lib/platform'

  let code = $state('')
  let nickname = $state('')
  let transfer = $state(false)
  let busy = $state(false)
  let error = $state<string | null>(null)

  async function submit(e?: SubmitEvent) {
    e?.preventDefault()
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

  // Phones and tablets: read the transfer code from the QR the other device shows in Settings.
  let scanning = $state(false)
  function scanned(text: string) {
    scanning = false
    const found = fromQr(text)
    if (!found) {
      error = "That QR isn't a Rynuvo transfer code."
      return
    }
    transfer = true
    code = found
    void submit()
  }
</script>

{#if scanning}<QrScanner onCode={scanned} onClose={(e) => ((scanning = false), (error = e ?? null))} />{/if}
<div class="wrap">
  <form class="card" onsubmit={submit}>
    <div class="logo"><Icon name="logo" size={30} /></div>
    <h1>Sign in to Rynuvo</h1>
    {#if store.notice}<p class="notice">{store.notice}</p>{/if}

    <label>
      {transfer ? 'Transfer code' : 'Invite code'}
      <input
        value={code}
        oninput={(e) => (code = e.currentTarget.value = formatCode(e.currentTarget.value))}
        placeholder="XXXX-XXXX-XXXX"
        maxlength="14"
        autocomplete="off"
        autocapitalize="characters"
        autocorrect="off"
        spellcheck="false"
        required
      />
    </label>
    {#if !transfer}
      <label>Nickname<input bind:value={nickname} required /></label>
    {/if}

    {#if error}<p class="err">{error}</p>{/if}
    <button class="btn primary" disabled={busy}>{busy ? 'Connecting…' : 'Sign in'}</button>
    {#if isMobile}
      <button type="button" class="btn" disabled={busy} onclick={() => ((error = null), (scanning = true))}>Scan QR from another device</button>
    {/if}
    <button type="button" class="link" onclick={() => ((transfer = !transfer), (error = null))}>
      {transfer ? 'Use an invite code' : 'Have a transfer code from another device?'}
    </button>
  </form>
</div>

<style>
  .wrap { height: 100%; display: grid; place-items: center; background: radial-gradient(70% 60% at 20% 10%, rgba(244, 184, 96, 0.12), transparent 60%), var(--desk); }
  .card { width: min(380px, calc(100vw - 32px)); box-sizing: border-box; background: var(--bg); border: 1px solid var(--line); border-radius: 22px; padding: 30px; display: flex; flex-direction: column; gap: 14px; box-shadow: 0 30px 80px rgba(0, 0, 0, 0.5); }
  .logo { color: var(--accent); display: flex; }
  h1 { margin: 2px 0 6px; font-size: 30px; font-weight: 750; font-stretch: 78%; letter-spacing: -0.01em; line-height: 1.05; font-variation-settings: 'opsz' 48; }
  label { display: flex; flex-direction: column; gap: 6px; font-size: 13px; font-weight: 650; color: var(--muted); }
  label input { font-weight: 400; font-size: 14.5px; padding: 11px 13px; }
  .err { color: var(--danger); margin: 0; font-size: 13px; }
  .notice { margin: 0; padding: 8px 10px; border-radius: 10px; background: rgba(242, 84, 91, 0.12); color: #fca5a5; font-size: 13px; }
  .btn.primary { padding: 11px 14px; margin-top: 4px; }
  .link { color: var(--accent); font-size: 13px; }
</style>
