<script lang="ts">
  import { onMount } from 'svelte'
  import { api, describeNet, errText, type TransferCode } from '../lib/api'
  import { store } from '../lib/state.svelte'
  import AdminPanel from './AdminPanel.svelte'
  import Countdown from './Countdown.svelte'
  import VoiceSettings from './VoiceSettings.svelte'

  let { onClose }: { onClose: () => void } = $props()

  let tab = $state<'general' | 'voice' | 'admin'>('general')
  let nickname = $state(store.me?.nickname ?? '')
  let nickMsg = $state<string | null>(null)
  let transfer = $state<TransferCode | null>(null)
  let net = $state<string[] | null>(null)
  let netBusy = $state(false)
  let logMsg = $state<string | null>(null)
  let version = $state('')
  let error = $state<string | null>(null)

  onMount(() => void api.appVersion().then((v) => (version = v)))

  async function rename(e: SubmitEvent) {
    e.preventDefault()
    try {
      store.me = await api.renameMe(nickname.trim())
      nickMsg = 'Saved'
    } catch (err) {
      nickMsg = errText(err)
    }
  }

  async function makeTransfer() {
    try {
      transfer = await api.transferCode()
    } catch (err) {
      error = errText(err)
    }
  }

  async function testNet() {
    netBusy = true
    net = null
    try {
      net = describeNet(await api.runNettest())
    } catch (err) {
      net = [errText(err)]
    } finally {
      netBusy = false
    }
  }

  async function sendLog() {
    logMsg = 'Sending…'
    try {
      await api.sendLog()
      logMsg = 'Sent to admin.'
    } catch (err) {
      logMsg = errText(err)
    }
  }
</script>

<svelte:window onkeydown={(e) => e.key === 'Escape' && !e.defaultPrevented && onClose()} />

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="overlay" onclick={onClose}>
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="panel" onclick={(e) => e.stopPropagation()}>
    <header>
      <nav>
        <button class:on={tab === 'general'} onclick={() => (tab = 'general')}>Settings</button>
        <button class:on={tab === 'voice'} onclick={() => (tab = 'voice')}>Voice</button>
        {#if store.me?.isAdmin}<button class:on={tab === 'admin'} onclick={() => (tab = 'admin')}>Admin</button>{/if}
      </nav>
      <button class="close" title="Close" onclick={onClose}>✕</button>
    </header>

    <div class="body">
      {#if tab === 'admin' && store.me?.isAdmin}
        <AdminPanel />
      {:else if tab === 'voice'}
        <VoiceSettings />
      {:else}
        <section>
          <h3>Nickname</h3>
          <form class="row" onsubmit={rename}>
            <input bind:value={nickname} />
            <button class="btn primary" disabled={!nickname.trim() || nickname.trim() === store.me?.nickname}>Save</button>
          </form>
          {#if nickMsg}<p class="note">{nickMsg}</p>{/if}
        </section>

        <section>
          <h3>Use Rynuvo on another device</h3>
          <p class="muted">Create a transfer code, then choose "Have a transfer code" on the other device.</p>
          {#if transfer}
            <div class="code selectable">{transfer.code}</div>
            <p class="note"><Countdown until={transfer.expiresAt} /></p>
          {/if}
          <button class="btn" onclick={makeTransfer}>{transfer ? 'Create a new code' : 'Create transfer code'}</button>
        </section>

        <section>
          <h3>Connection test</h3>
          <p class="muted">Checks whether calls and screen share can connect directly to your friends.</p>
          {#if net}<ul>{#each net as line}<li>{line}</li>{/each}</ul>{/if}
          <button class="btn" disabled={netBusy} onclick={testNet}>{netBusy ? 'Testing…' : 'Test connection'}</button>
        </section>

        <section>
          <h3>Having problems?</h3>
          <button class="btn" onclick={sendLog}>Send log to admin</button>
          {#if logMsg}<p class="note">{logMsg}</p>{/if}
        </section>

        {#if error}<p class="err">{error}</p>{/if}

        <footer>
          <span class="muted">Rynuvo {version}</span>
          <button class="btn danger" onclick={() => store.logout()}>Sign out of this device</button>
        </footer>
      {/if}
    </div>
  </div>
</div>

<style>
  .overlay { position: fixed; inset: 0; background: rgba(8, 8, 12, 0.7); display: grid; place-items: center; z-index: 40; }
  .panel { width: min(560px, 92vw); max-height: 86vh; display: flex; flex-direction: column; background: var(--panel); border: 1px solid var(--line); border-radius: 14px; }
  header { display: flex; align-items: center; justify-content: space-between; padding: 10px 14px; border-bottom: 1px solid var(--line); }
  nav { display: flex; gap: 4px; }
  nav button { padding: 6px 12px; border-radius: 8px; color: var(--muted); }
  nav button.on { background: var(--elev); color: var(--text); }
  .close { color: var(--muted); padding: 4px 8px; }
  .body { overflow-y: auto; padding: 6px 20px 18px; }
  section { padding: 14px 0; border-bottom: 1px solid var(--line); display: flex; flex-direction: column; gap: 8px; align-items: flex-start; }
  h3 { margin: 0; font-size: 13px; text-transform: uppercase; letter-spacing: 0.04em; color: var(--muted); }
  p { margin: 0; font-size: 13px; }
  .row { display: flex; gap: 8px; width: 100%; }
  .row input { flex: 1; }
  .code { font-family: var(--mono); font-size: 22px; letter-spacing: 0.08em; color: #fff; }
  .note { color: var(--muted); }
  ul { margin: 0; padding-left: 18px; font-size: 13px; }
  .err { color: var(--danger); }
  footer { display: flex; justify-content: space-between; align-items: center; padding-top: 14px; }
</style>
