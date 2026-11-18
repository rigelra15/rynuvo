<script lang="ts">
  import DiamondQr from './DiamondQr.svelte'
  import { fly } from 'svelte/transition'
  import { toQr } from '../lib/code'
  import { backdrop, sheet } from '../lib/motion'
  import Icon from './Icon.svelte'
  import { PRESETS, selfCheckLines } from '../lib/stream'
  import { AVATAR_ICONS, PALETTE, avatarColor, isHex } from '../lib/avatar'
  import { imageColor } from '../lib/imageColor'
  import { AVATAR_SHAPES } from '../lib/avatarIcons'
  import Avatar from './Avatar.svelte'
  import { onMount } from 'svelte'
  import { api, describeNet, errText, mediaUrl, type TransferCode } from '../lib/api'
  import type { ProfilePatch } from '../lib/types'
  import { store } from '../lib/state.svelte'
  import AdminPanel from './AdminPanel.svelte'
  import Countdown from './Countdown.svelte'
  import VoiceSettings from './VoiceSettings.svelte'
  import { updater } from '../lib/updater.svelte'
  import { isAndroid, isMobile } from '../lib/platform'
  import { bubble } from '../lib/bubble.svelte'
  import { overlayEnabled, setOverlayEnabled } from '../lib/overlay'
  import { updateStatus, type CheckResult } from '../lib/update'

  let { onClose }: { onClose: () => void } = $props()

  let tab = $state<'general' | 'voice' | 'admin'>('general')
  let nickname = $state(store.me?.nickname ?? '')
  let nickMsg = $state<string | null>(null)
  let transfer = $state<TransferCode | null>(null)
  // A code shown here ends one of two ways: used to sign in a device (the server says so), or
  // run out of time. Either way it closes instead of sitting there looking usable.
  let expired = $state(false)
  $effect(() => {
    if (!transfer) return
    expired = false
    const t = setTimeout(() => (expired = true), Math.max(0, transfer.expiresAt - Date.now()))
    return () => clearTimeout(t)
  })
  const claimed = $derived(transfer && store.transferClaimed?.expiresAt === transfer.expiresAt ? store.transferClaimed : null)
  const transferState = $derived<'live' | 'done' | 'expired'>(claimed ? 'done' : expired ? 'expired' : 'live')
  let net = $state<string[] | null>(null)
  let netBusy = $state(false)
  let logMsg = $state<string | null>(null)
  let diagnosticMsg = $state<string | null>(null)
  let version = $state('')
  let error = $state<string | null>(null)
  let checkingUpdate = $state(false)
  let updateResult = $state<CheckResult | null>(null)
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
  async function pick(p: ProfilePatch) {
    try {
      await store.setAvatar(p)
      avatarErr = null
    } catch (err) {
      avatarErr = errText(err)
    }
  }

  // A photo from this computer: compressed and uploaded like a chat image, then set as my avatar.
  let photoInput = $state<HTMLInputElement>()
  let uploading = $state(false)
  async function uploadPhoto() {
    if (!photoInput) return
    const file = photoInput.files?.[0]
    photoInput.value = ''
    if (!file) return
    uploading = true
    try {
      const ref = await api.uploadImage(new Uint8Array(await file.arrayBuffer()))
      // A new photo brings its own colour (the first swatch); any colour can still be picked after.
      const color = await imageColor(file).catch(() => null)
      if (color) photoColor = { id: ref.id, hex: color }
      await pick(color ? { avatarImage: ref.id, avatarColor: color } : { avatarImage: ref.id })
    } catch (err) {
      avatarErr = errText(err)
    } finally {
      uploading = false
    }
  }

  // The colour of my current photo, shown as the first swatch.
  let photoColor = $state<{ id: string; hex: string } | null>(null)
  $effect(() => {
    const id = store.me?.avatarImage
    if (!id || photoColor?.id === id) return
    imageColor(mediaUrl(id)).then((hex) => {
      photoColor = { id, hex }
      // A photo set before colours followed photos, and no colour ever picked: follow it now.
      if (store.me && !store.me.avatarColor && store.me.avatarImage === id) void pick({ avatarColor: hex })
    }, () => {})
  })
  const myColor = $derived(store.me ? avatarColor(store.me) : '')
  // A custom colour that's neither a palette shade nor the photo's: the picker swatch shows it.
  const customColor = $derived(isHex(store.me?.avatarColor) && store.me.avatarColor !== photoColor?.hex ? store.me.avatarColor : null)
  let colorTimer: ReturnType<typeof setTimeout> | undefined
  function pickCustom(e: Event) {
    const hex = (e.currentTarget as HTMLInputElement).value.toLowerCase()
    // Dragging around the picker fires many inputs: save once it settles.
    clearTimeout(colorTimer)
    colorTimer = setTimeout(() => pick({ avatarColor: hex }), 250)
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

  async function exportDiagnostics() {
    diagnosticMsg = 'Exporting…'
    try {
      diagnosticMsg = `Saved to ${await api.exportStreamDiagnostics()}`
    } catch (err) {
      diagnosticMsg = errText(err)
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
  <div class="panel" transition:sheet|global onclick={(e) => e.stopPropagation()}>
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
              <div class="photo">
                <input bind:this={photoInput} type="file" accept="image/png,image/jpeg,image/webp,image/gif" hidden onchange={uploadPhoto} />
                <button class="btn" disabled={uploading} onclick={() => photoInput?.click()}>{uploading ? 'Uploading…' : 'Upload photo'}</button>
                {#if store.me?.avatarImage}<button class="btn" disabled={uploading} onclick={() => pick({ avatarImage: null })}>Remove photo</button>{/if}
              </div>
              <div class="icons">
                <button class="opt" class:sel={!store.me?.avatarIcon && !store.me?.avatarImage} title="Letter" onclick={() => pick({ avatarIcon: null, avatarImage: null })}>
                  <span class="letter">{store.me?.nickname.slice(0, 1).toUpperCase()}</span>
                </button>
                {#each AVATAR_ICONS as name (name)}
                  <button class="opt" class:sel={!store.me?.avatarImage && store.me?.avatarIcon === name} title={name} onclick={() => pick({ avatarIcon: name, avatarImage: null })}>
                    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                      {#each AVATAR_SHAPES[name] as sh, i (i)}
                        {#if 'd' in sh}<path d={sh.d} />{:else if 'c' in sh}<circle cx={sh.c[0]} cy={sh.c[1]} r={sh.c[2]} />{:else}<ellipse cx={sh.e[0]} cy={sh.e[1]} rx={sh.e[2]} ry={sh.e[3]} />{/if}
                      {/each}
                    </svg>
                  </button>
                {/each}
              </div>
              <div class="swatches">
                {#if store.me?.avatarImage && photoColor?.id === store.me.avatarImage}
                  {@const hex = photoColor.hex}
                  <button class="sw photo" class:sel={myColor === hex} style="--c:{hex}" title="From your photo" aria-label="Colour from your photo" onclick={() => pick({ avatarColor: hex })}>
                    <Icon name="image" size={12} />
                  </button>
                {/if}
                {#each Object.entries(PALETTE) as [key, hex] (key)}
                  <button
                    class="sw"
                    class:sel={myColor === hex && !isHex(store.me?.avatarColor)}
                    style="background:{hex}"
                    title={key}
                    aria-label="{key} colour"
                    onclick={() => pick({ avatarColor: key })}
                  ></button>
                {/each}
                <label class="sw custom" class:sel={!!customColor} style={customColor ? `background:${customColor}` : ''} title="Pick any colour">
                  <input type="color" value={myColor} oninput={pickCustom} aria-label="Pick any colour" />
                  {#if !customColor}<Icon name="plus" size={12} />{/if}
                </label>
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
          <p class="muted">Create a transfer code, then scan it or type it in on the other device.</p>
          {#if transfer}
            <div class="transfer" class:done={transferState === 'done'} class:expired={transferState === 'expired'}>
              <div class="qr"><DiamondQr text={toQr(transfer.code)} state={transferState} device={claimed?.device} label="QR code for this transfer code" /></div>
              <div class="tinfo">
                <div class="code" class:selectable={transferState === 'live'}>{transfer.code}</div>
                {#if transferState === 'done'}
                  <p class="claimed" in:fly={{ y: 6, duration: 360, delay: 700 }}>Signed in on {claimed?.device || 'another device'}</p>
                  <p class="muted">This code is used up. Make a new one for another device.</p>
                {:else if transferState === 'expired'}
                  <p class="note">Expired</p>
                  <p class="muted">Codes last 10 minutes. Make a new one to try again.</p>
                {:else}
                  <p class="note"><Countdown until={transfer.expiresAt} /></p>
                  <p class="muted">On a phone or tablet, tap "Scan QR" on the sign-in screen.</p>
                {/if}
              </div>
            </div>
          {/if}
          <button class="btn" onclick={makeTransfer}>{transfer && transferState === 'live' ? 'Create a new code' : 'Create transfer code'}</button>
        </section>

        <section>
          <h3>Connection test</h3>
          <p class="muted">Checks whether calls and screen share can connect directly to your friends.</p>
          {#if net}<ul>{#each net as line}<li>{line}</li>{/each}</ul>{/if}
          <button class="btn" disabled={netBusy} onclick={testNet}>{netBusy ? 'Testing…' : 'Test connection'}</button>
        </section>

        <!-- Phones and tablets can't share their screen, start with the computer, or take hotkeys. -->
        {#if !isMobile}
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
        {/if}

        {#if !isMobile}
        <section>
          <h3>Call overlay</h3>
          <label class="chk"><input type="checkbox" checked={overlayEnabled()} onchange={(e) => setOverlayEnabled(e.currentTarget.checked)} /> Show who's talking over games</label>
          <p class="note">While you're in a room and playing, a small panel shows on top of the game (windowed or borderless; not exclusive full screen). Clicks pass through it; press {navigator.userAgent.includes('Mac') ? '⌘⇧O' : 'Ctrl+Shift+O'} to use its buttons or move it.</p>
        </section>
        {/if}

        {#if bubble.status}
        <section>
          <h3>Call bubble</h3>
          <label class="chk"><input type="checkbox" checked={bubble.status === 'on'} onchange={(e) => bubble.set(e.currentTarget.checked)} /> Float the call over other apps</label>
          <p class="note">While you're in a room and switch to another app, a bubble shows who's talking, with mute and leave.{#if bubble.status === 'permission'} Android asks you to allow Rynuvo to display over other apps.{/if}</p>
        </section>
        {/if}

        {#if !isMobile || isAndroid}
        <section>
          <h3>Updates</h3>
          <p class="muted">
            You're on Rynuvo {version}.
            {isAndroid ? 'New versions download when Rynuvo opens; Android asks you to confirm the install.' : 'New versions download in the background on their own.'}
          </p>
          <div class="row">
            <button class="btn" onclick={checkUpdates} disabled={checkingUpdate || updater.state.phase !== 'idle'}>Check for updates</button>
          </div>
          {#if upd.text}<p class="note" aria-live="polite">{upd.text}</p>{/if}
        </section>
        {/if}

        {#if !isMobile}
        <section>
          <h3>Startup</h3>
          <label class="chk"><input type="checkbox" bind:checked={autostart} onchange={setAutostart} /> Open Rynuvo when I sign in to this computer</label>
        </section>
        {/if}

        <section>
          <h3>Having problems?</h3>
          {#if !isMobile}
            <button class="btn" onclick={exportDiagnostics}>Export screen share diagnostics</button>
            {#if diagnosticMsg}<p class="note">{diagnosticMsg}</p>{/if}
          {/if}
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
  .photo { display: flex; gap: 8px; margin-bottom: 8px; }
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
  .transfer { display: flex; gap: 16px; align-items: center; flex-wrap: wrap; }
  .tinfo .code { transition: opacity 400ms, text-decoration-color 400ms; }
  .transfer:not(.done):not(.expired) .tinfo .code { text-decoration: line-through transparent; }
  .transfer.done .tinfo .code, .transfer.expired .tinfo .code { opacity: 0.35; text-decoration: line-through; }
  .claimed { margin: 6px 0 2px; color: var(--speaking); font-weight: 700; }
  /* The code inside the diamond is ~70% of this: still larger than the old square QR. */
  .qr { width: 220px; height: 220px; flex: none; }
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
  /* My photo's colour: first, marked with a little picture. */
  .sw.photo { display: grid; place-items: center; background: var(--c); color: rgba(255, 255, 255, 0.92); }
  /* Any colour: a colour wheel until one is picked, then that colour. */
  .sw.custom { position: relative; display: grid; place-items: center; cursor: pointer; color: #fff; background: conic-gradient(#ef4444, #eab308, #22c55e, #06b6d4, #3b82f6, #a855f7, #ef4444); }
  .sw.custom input { position: absolute; inset: 0; width: 100%; height: 100%; opacity: 0; cursor: pointer; border: 0; padding: 0; }
  .sw.custom :global(svg) { filter: drop-shadow(0 0 2px rgba(0, 0, 0, 0.6)); pointer-events: none; }
  .chk { display: flex; gap: 8px; align-items: center; font-size: 13px; }
  footer { display: flex; justify-content: space-between; align-items: center; padding-top: 14px; }
  /* Phones: a full screen page instead of a floating panel. */
  @media (max-width: 760px) {
    .overlay { padding: env(safe-area-inset-top) 0 env(safe-area-inset-bottom); }
    .panel { width: 100%; height: 100%; max-height: none; border-radius: 0; border: 0; }
    .profile { flex-wrap: wrap; }
    .icons { grid-template-columns: repeat(auto-fill, 32px); }
  }
</style>
