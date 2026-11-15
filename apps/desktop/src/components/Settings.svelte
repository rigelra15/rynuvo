<script lang="ts">
  import { backdrop, pop } from '../lib/motion'
  import Icon from './Icon.svelte'
  import { PRESETS, selfCheckLines } from '../lib/stream'
  import { AVATAR_ICONS, PALETTE, avatarColor } from '../lib/avatar'
  import { AVATAR_SHAPES } from '../lib/avatarIcons'
  import Avatar from './Avatar.svelte'
  import { onMount } from 'svelte'
  import { api, describeNet, errText, type TransferCode } from '../lib/api'
  import { store } from '../lib/state.svelte'
  import AdminPanel from './AdminPanel.svelte'
  import Countdown from './Countdown.svelte'
  import VoiceSettings from './VoiceSettings.svelte'
  import { updater } from '../lib/updater.svelte'
  import { updateStatus, type CheckResult } from '../lib/update'

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
  let checkingUpdate = $state(false)
  let updateResult = $state<CheckResult | null>(null)
  let restarting = $state(false)
  const upd = $derived(
    updateStatus({ update: updater.state, checking: checkingUpdate, result: updateResult, busy: store.myVoice.channelId !== null || store.live !== null }),
  )

  async function checkUpdates() {
    checkingUpdate = true
    updateResult = null
    try {
      updateResult = await updater.checkNow()
    } finally {
      checkingUpdate = false
    }
  }

  async function restartToUpdate() {
    restarting = true
    try {
      await updater.restart()
    } finally {
      restarting = false
    }
  }

  let check = $state<ReturnType<typeof selfCheckLines> | null>(null)
  let captureDenied = $state(false)
  let checking = $state(false)

  async function runCheck() {
    checking = true
    try {
      const c = await api.streamSelfCheck()
      check = selfCheckLines(c)
      captureDenied = c.capture === 'denied'
    } catch (err) {
      error = errText(err)
    } finally {
      checking = false
    }
  }

  let avatarErr = $state<string | null>(null)
  async function pick(p: { avatarIcon?: string | null; avatarColor?: string | null }) {
    try {
      await store.setAvatar(p)
      avatarErr = null
    } catch (err) {
      avatarErr = errText(err)
    }
  }

  let autostart = $state(false)
  onMount(() => {
    void api.appVersion().then((v) => (version = v))
    void api.prefsGet().then((p) => (autostart = p.autostart))
  })

  async function setPreset() {
    try {
      const p = await api.prefsGet()
      store.streamPreset = (await api.prefsSet({ ...p, streamPreset: store.streamPreset })).streamPreset
    } catch (err) {
      error = errText(err)
    }
  }

  async function setAutostart() {
    try {
      const p = await api.prefsGet()
      autostart = (await api.prefsSet({ ...p, autostart })).autostart
    } catch (err) {
      autostart = !autostart
      error = errText(err)
    }
  }

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
<div class="overlay" transition:backdrop|global onclick={onClose}>
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="panel" transition:pop|global onclick={(e) => e.stopPropagation()}>
    <header>
      <nav>
        <button class:on={tab === 'general'} onclick={() => (tab = 'general')}>Settings</button>
        <button class:on={tab === 'voice'} onclick={() => (tab = 'voice')}>Voice</button>
        {#if store.me?.isAdmin}<button class:on={tab === 'admin'} onclick={() => (tab = 'admin')}>Admin</button>{/if}
      </nav>
      <button class="close" title="Close" onclick={onClose}><Icon name="close" size={18} /></button>
    </header>

    <div class="body">
      {#if tab === 'admin' && store.me?.isAdmin}
        <AdminPanel />
      {:else if tab === 'voice'}
        <VoiceSettings />
      {:else}
        <section>
          <h3>Profile</h3>
          <div class="profile">
            <Avatar user={store.me} size={56} />
            <div class="pick">
              <div class="icons">
                <button class="opt" class:sel={!store.me?.avatarIcon} title="Letter" onclick={() => pick({ avatarIcon: null })}>
                  <span class="letter">{store.me?.nickname.slice(0, 1).toUpperCase()}</span>
                </button>
                {#each AVATAR_ICONS as name (name)}
                  <button class="opt" class:sel={store.me?.avatarIcon === name} title={name} onclick={() => pick({ avatarIcon: name })}>
                    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                      {#each AVATAR_SHAPES[name] as sh, i (i)}
                        {#if 'd' in sh}<path d={sh.d} />{:else if 'c' in sh}<circle cx={sh.c[0]} cy={sh.c[1]} r={sh.c[2]} />{:else}<ellipse cx={sh.e[0]} cy={sh.e[1]} rx={sh.e[2]} ry={sh.e[3]} />{/if}
                      {/each}
                    </svg>
                  </button>
                {/each}
              </div>
              <div class="swatches">
                {#each Object.entries(PALETTE) as [key, hex] (key)}
                  <button
                    class="sw"
                    class:sel={store.me && avatarColor(store.me) === hex}
                    style="background:{hex}"
                    title={key}
                    aria-label="{key} colour"
                    onclick={() => pick({ avatarColor: key })}
                  ></button>
                {/each}
              </div>
            </div>
          </div>
          {#if avatarErr}<p class="err">{avatarErr}</p>{/if}
        </section>

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
          <h3>Screen share</h3>
          <label class="chk">
            Default quality
            <select bind:value={store.streamPreset} onchange={setPreset}>
              {#each PRESETS as p (p.id)}<option value={p.id}>{p.label}</option>{/each}
            </select>
          </label>
          <button class="btn" onclick={runCheck} disabled={checking}>{checking ? 'Checking…' : 'Check screen sharing'}</button>
          {#if check}
            <ul class="check">
              {#each check as l (l.label)}
                <li class:bad={!l.ok}><span class="cl"><Icon name={l.ok ? 'check' : 'close'} size={14} /> {l.label}</span><span class="note">{l.detail}</span></li>
              {/each}
            </ul>
            {#if captureDenied}
              <button class="btn" onclick={() => void api.openCaptureSettings()}>Open Screen Recording settings</button>
            {/if}
          {/if}
        </section>

        <section>
          <h3>Updates</h3>
          <p class="muted">You're on Rynuvo {version}. New versions download in the background on their own.</p>
          <div class="row">
            {#if upd.restart}
              <button class="btn primary" onclick={restartToUpdate} disabled={restarting}>{restarting ? 'Restarting…' : 'Restart to update'}</button>
            {:else}
              <button class="btn" onclick={checkUpdates} disabled={checkingUpdate || updater.state.phase !== 'idle'}>Check for updates</button>
            {/if}
          </div>
          {#if upd.text}<p class="note" aria-live="polite">{upd.text}</p>{/if}
        </section>

        <section>
          <h3>Startup</h3>
          <label class="chk"><input type="checkbox" bind:checked={autostart} onchange={setAutostart} /> Open Rynuvo when I sign in to this computer</label>
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
  .panel { width: min(580px, 92vw); max-height: 86vh; display: flex; flex-direction: column; background: var(--bg); border: 1px solid var(--line); border-radius: 22px; box-shadow: 0 30px 80px rgba(0, 0, 0, 0.5); overflow: hidden; }
  header { display: flex; align-items: center; justify-content: space-between; padding: 12px 14px 12px 16px; border-bottom: 1px solid var(--line); }
  nav { display: flex; gap: 4px; }
  nav button { padding: 6px 12px; border-radius: 10px; color: var(--muted); font-weight: 600; }
  nav button.on { background: var(--elev); color: var(--text); box-shadow: inset 0 -2px 0 var(--accent); }
  .close { color: var(--muted); padding: 4px; display: grid; place-items: center; border-radius: 6px; }
  .body { overflow-y: auto; padding: 6px 20px 18px; }
  section { padding: 14px 0; border-bottom: 1px solid var(--line); display: flex; flex-direction: column; gap: 8px; align-items: flex-start; }
  h3 { margin: 0; font-size: 15px; font-weight: 700; color: var(--text); }
  p { margin: 0; font-size: 13px; }
  .row { display: flex; gap: 8px; width: 100%; }
  .row input { flex: 1; }
  .code { font-family: var(--mono); font-size: 22px; letter-spacing: 0.08em; color: var(--accent); }
  .note { color: var(--muted); }
  ul { margin: 0; padding-left: 18px; font-size: 13px; }
  .err { color: var(--danger); }
  .check { list-style: none; margin: 8px 0; padding: 0; display: grid; gap: 4px; font-size: 13px; }
  .check li { display: flex; justify-content: space-between; gap: 12px; color: #86efac; }
  .check li.bad { color: var(--danger); }
  .cl { display: inline-flex; gap: 6px; align-items: center; }
  .profile { display: flex; gap: 16px; align-items: flex-start; }
  .pick { display: grid; gap: 10px; flex: 1; }
  .icons { display: grid; grid-template-columns: repeat(9, 32px); gap: 6px; }
  .opt { width: 32px; height: 32px; border-radius: 10px; display: grid; place-items: center; color: var(--muted); background: var(--elev-2); }
  .opt:hover { color: var(--text); background: var(--elev); }
  .opt.sel { color: var(--text); box-shadow: inset 0 0 0 2px var(--accent); }
  .letter { font-weight: 700; font-size: 14px; }
  .swatches { display: flex; flex-wrap: wrap; gap: 8px; }
  .sw { width: 24px; height: 24px; border-radius: 50%; box-shadow: inset 0 0 0 2px rgba(255, 255, 255, 0.12); }
  .sw.sel { box-shadow: 0 0 0 2px var(--panel), 0 0 0 4px #fff; }
  .chk { display: flex; gap: 8px; align-items: center; font-size: 13px; }
  footer { display: flex; justify-content: space-between; align-items: center; padding-top: 14px; }
</style>
