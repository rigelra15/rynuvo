<script lang="ts">
  import HomeView from './components/HomeView.svelte'
  import Welcome from './components/Welcome.svelte'
  import { markWelcomed, permissions, welcomed } from './lib/permissions.svelte'
  import { isAndroid, isMobile } from './lib/platform'
  import Splash from './components/Splash.svelte'
  import { onMount, untrack } from 'svelte'
  import ChannelMenu from './components/ChannelMenu.svelte'
  import ChannelView from './components/ChannelView.svelte'
  import ConnBanner from './components/ConnBanner.svelte'
  import InviteToast from './components/InviteToast.svelte'
  import CreateChannelDialog from './components/CreateChannelDialog.svelte'
  import Onboarding from './components/Onboarding.svelte'
  import Settings from './components/Settings.svelte'
  import DialogHost from './components/DialogHost.svelte'
  import { dialog } from './lib/dialog.svelte'
  import Sidebar from './components/Sidebar.svelte'
  import UserVolumeMenu from './components/UserVolumeMenu.svelte'
  import VerifyDialog from './components/VerifyDialog.svelte'
  import { api, errText, onE2eeChanged, onMusicEvent, onSessionEvent, onStreamEvent, onVoiceEvent } from './lib/api'
  import { store } from './lib/state.svelte'
  import { updater } from './lib/updater.svelte'
  import { layout } from './lib/layout.svelte'
  import { pane } from './lib/pane.svelte'
  import { callNotice } from './lib/callNotice'
  import { bubble } from './lib/bubble.svelte'
  import { overlay } from './lib/overlay'
  import { watchActivity } from './lib/activity'
  import { startPush } from './lib/push'
  import { tick } from 'svelte'
  import type { Channel, ChannelKind, Id } from './lib/types'

  let creating = $state<ChannelKind | null>(null)
  let renaming = $state<Channel | null>(null)
  let menu = $state<{ c: Channel; x: number; y: number } | null>(null)
  let settingsOpen = $state(false)
  // The splash plays its opening (the diamond coming together, the name) even when loading is quicker.
  let introDone = $state(matchMedia('(prefers-reduced-motion: reduce)').matches)
  setTimeout(() => (introDone = true), 2800)
  // A new version found while the app is open takes the screen once it's downloaded (the
  // splash, as the update screen). At launch the splash already shows the whole update.
  // Unread: the open chat counts as read only while it's actually on screen.
  let shown = $state(document.visibilityState === 'visible')
  $effect(() => {
    const on = () => (shown = document.visibilityState === 'visible')
    document.addEventListener('visibilitychange', on)
    return () => document.removeEventListener('visibilitychange', on)
  })
  $effect(() => {
    store.looking = shown && (!narrow || paneOpen)
  })
  $effect(() => store.seeSelected())

  // No connection at launch: the splash waits, and after a moment says why.
  let online = $state(navigator.onLine)
  $effect(() => {
    const on = () => (online = true)
    const off = () => (online = false)
    window.addEventListener('online', on)
    window.addEventListener('offline', off)
    return () => {
      window.removeEventListener('online', on)
      window.removeEventListener('offline', off)
    }
  })
  // Lost the connection inside the app: the colour drains out, slowly, and comes back when it does.
  // A blip (a server restart, a quick network switch) doesn't count: only after a moment offline.
  const disconnected = $derived(store.phase === 'app' && store.gotReady && (store.conn.state !== 'online' || !online))
  let mono = $state(false)
  $effect(() => {
    if (!disconnected) {
      mono = false
      return
    }
    const t = setTimeout(() => (mono = true), 1200)
    return () => clearTimeout(t)
  })
  const waiting = $derived(store.phase === 'app' && !store.ready && store.bootSlow ? (online ? 'server' : 'offline') : null)
  const updateCover = $derived(['ready', 'installing', 'install', 'failed'].includes(updater.state.phase))
  const splashUp = $derived(!store.ready || !introDone || updater.boot || updateCover)
  // First launch on Android: after the splash (and its update check) come the welcome screens, which
  // ask for the mic, notifications and installing updates, and then sign-in. Someone already signed
  // in with everything allowed (an update from before these screens) goes straight in.
  let welcoming = $state(isAndroid && !welcomed())
  const showWelcome = $derived(welcoming && !splashUp && store.phase !== 'boot')
  $effect(() => {
    if (showWelcome && store.phase === 'app' && permissions.allGranted()) finishWelcome()
  })
  function finishWelcome() {
    markWelcomed()
    welcoming = false
  }
  // Later launches: ask again for what's still missing (Android only shows what isn't granted).
  let askedPermissions = false
  $effect(() => {
    if (splashUp || welcoming || askedPermissions) return
    askedPermissions = true
    const bridge = (window as unknown as { RynuvoAndroid?: { requestPermissions?: () => void } }).RynuvoAndroid
    if (bridge?.requestPermissions) setTimeout(() => bridge.requestPermissions?.(), 750)
  })
  let memberMenu = $state<{ userId: Id; x: number; y: number } | null>(null)

  // Phones: one pane at a time. The list of chats and rooms is home; opening one covers it, and
  // Back (the button or the system back gesture, through history) returns to the list.
  const narrowQuery = window.matchMedia('(max-width: 760px)')
  let narrow = $state(narrowQuery.matches)
  let paneOpen = $state(false)
  $effect(() => {
    const on = () => (narrow = narrowQuery.matches)
    const pop = () => {
      // Still on the open pane's entry (something inside it, like a full-screen stream, closed).
      if (history.state?.pane) return
      paneOpen = false
      // Leaving the Friends page lands back on the list with Rooms selected, so Friends opens again.
      if (store.view === 'home') store.view = 'circle'
    }
    narrowQuery.addEventListener('change', on)
    window.addEventListener('popstate', pop)
    return () => {
      narrowQuery.removeEventListener('change', on)
      window.removeEventListener('popstate', pop)
    }
  })
  // A room rises from the bottom like a sheet (and is swiped back down); chats slide in from the right.
  let sheet = $state(false)
  let mainEl: HTMLElement | undefined = $state()
  /** Open a chat or room from the sidebar. */
  function goTo(c: Channel) {
    store.view = 'circle'
    store.select(c.id)
    // Opening a room I'm already in on another device shows it; moving the call here is a choice.
    if (c.kind === 'voice' && !store.inVoiceElsewhere(c.id)) store.joinVoice(c.id)
    void openPane()
  }

  async function openPane() {
    if (!narrow || paneOpen) return
    const next = store.view !== 'home' && store.selected?.kind === 'voice'
    history.pushState({ pane: true }, '')
    // The closed position (below or to the right) jumps into place, unseen, before the slide starts.
    if (next !== sheet && mainEl) {
      mainEl.style.transition = 'none'
      sheet = next
      await tick()
      void mainEl.offsetWidth
      mainEl.style.transition = ''
    }
    paneOpen = true
  }
  const closePane = () => history.back()
  $effect(() => {
    pane.narrow = narrow
    pane.close = closePane
  })

  // Swipe a room down to put it away: it follows the finger, and goes if pulled far or flicked.
  let drag = $state<number | null>(null)
  function dragStart(e: TouchEvent) {
    if (!narrow || !paneOpen || !sheet || e.touches.length !== 1) return
    const t = e.target as Element
    if (!t.closest('.top, .room') || t.closest('input, textarea, .codepop')) return
    const y0 = e.touches[0].clientY
    const x0 = e.touches[0].clientX
    const t0 = performance.now()
    let dy = 0
    let live = false
    const move = (ev: TouchEvent) => {
      dy = ev.touches[0].clientY - y0
      if (!live) {
        if (Math.abs(ev.touches[0].clientX - x0) > 12) return end()
        if (dy < 10) return
        live = true
      }
      ev.preventDefault()
      drag = Math.max(0, dy)
    }
    const end = () => {
      window.removeEventListener('touchmove', move)
      window.removeEventListener('touchend', end)
      window.removeEventListener('touchcancel', end)
      if (!live) return
      const fast = dy / (performance.now() - t0) > 0.6
      drag = null
      if (dy > 140 || (fast && dy > 40)) closePane()
    }
    window.addEventListener('touchmove', move, { passive: false })
    window.addEventListener('touchend', end)
    window.addEventListener('touchcancel', end)
  }
  // The Friends tab is a page of its own on a phone.
  $effect(() => {
    if (store.view === 'home') openPane()
  })

  // Android: the call notification follows my call (room, mute, deafen) and keeps it alive in the background.
  const syncCall = callNotice(store)
  bubble.init()
  $effect(() => {
    // (It reads the room's people and who's talking too, so it reruns on those.)
    void [store.myVoice.channelId, store.myVoice.muted, store.myVoice.deafened, store.channels]
    syncCall?.()
  })

  // Desktop: the call overlay over other apps while Rynuvo is in the background.
  const syncOverlay = isMobile ? null : overlay(store)
  // Desktop: the game I'm playing, shown to others.
  if (!isMobile) watchActivity()
  $effect(() => {
    syncOverlay?.()
  })

  $effect(() => {
    const n = store.streamNotice
    if (n) {
      store.streamNotice = null
      void dialog.notice(n)
    }
  })

  $effect(() => {
    const n = store.voiceNotice
    if (n) {
      store.voiceNotice = null
      void dialog.notice(n)
    }
  })

  $effect(() => {
    if (store.phase === 'onboarding') settingsOpen = false
  })

  // Android: notifications while the app is closed, and opening what a tapped one is about.
  $effect(() => {
    if (store.phase !== 'app') return
    // Only the phase decides when this runs, not what opening a chat happens to read.
    return untrack(() =>
      startPush((id) => {
        const c = store.channels.find((c) => c.id === id)
        if (c) goTo(c)
      }),
    )
  })

  // Back online (woke up, network came back): a good moment to look for a new version.
  $effect(() => {
    if (store.conn.state === 'online') updater.nudge()
  })

  onMount(() => {
    updater.start()
    const un = onSessionEvent((e) => store.apply(e))
    const unv = onVoiceEvent((e) => store.applyVoiceEvent(e))
    const uns = onStreamEvent((e) => store.applyStreamEvent(e))
    const une = onE2eeChanged(() => void store.refreshSecurity())
    const unm = onMusicEvent((e) => store.musicStatus.set(e.videoId, e))
    void store.boot()
    return () => {
      void un.then((f) => f())
      void unv.then((f) => f())
      void uns.then((f) => f())
      void une.then((f) => f())
      void unm.then((f) => f())
    }
  })

  async function create(name: string) {
    const c = await api.createChannel(creating!, name)
    store.apply({ kind: 'server', data: { t: 'channelCreated', d: c } })
    store.select(c.id)
  }

  async function del(c: Channel) {
    menu = null
    if (!(await dialog.ask(`Delete "${c.name}"${c.kind === 'text' ? ' and all of its messages' : ''}?`, 'Delete'))) return
    try {
      await api.deleteChannel(c.id)
    } catch (e) {
      void dialog.notice(errText(e))
    }
  }
</script>

<!-- Offline: everything under the status bar fades to grey (the bar stays in colour). -->
<div class="mono" class:on={mono} aria-hidden="true"></div>

{#if showWelcome}<Welcome signedIn={store.phase === 'app'} onDone={finishWelcome} />{/if}
{#if splashUp}<Splash update={updater.state.phase === 'idle' ? null : updater.state} {waiting} />{/if}

{#if store.phase === 'boot'}
  <!-- the splash covers boot -->
{:else if store.phase === 'onboarding'}
  <Onboarding />
{:else if store.me}
  <div class="app">
    <ConnBanner />
    <div class="shell" class:narrow class:sheet class:open={narrow && paneOpen} class:tucked={!narrow && layout.sideHidden && store.view !== 'home' && store.selected?.kind === 'voice'}>
      <Sidebar
        channels={store.sortedChannels()}
        selectedId={narrow && !paneOpen ? null : store.selectedId}
        me={store.me}
        canManage={store.canManage}
        onSelect={goTo}
        onMemberMenu={(userId, e) => (memberMenu = { userId, x: e.clientX, y: e.clientY })}
        onCreate={(k) => (creating = k)}
        onChannelMenu={(c, e) => (menu = { c, x: e.clientX, y: e.clientY })}
        onSettings={() => (settingsOpen = true)}
      />
      <main class="main" bind:this={mainEl} class:dragging={drag !== null} style:transform={drag !== null ? `translateY(${drag}px)` : null} ontouchstart={dragStart}>
        <div class="pane">
        {#if store.view === 'home'}
          <HomeView onMemberMenu={(userId, e) => (memberMenu = { userId, x: e.clientX, y: e.clientY })} />
        {:else if store.selected}
          {#key store.selected.id}
            <ChannelView channel={store.selected} onMemberMenu={(userId, e) => (memberMenu = { userId, x: e.clientX, y: e.clientY })} />
          {/key}
        {:else}
          <div class="none muted">No chats yet. Click + next to "Chats" to start one.</div>
        {/if}
        </div>
      </main>
    </div>
  </div>
{:else}
  <div class="app"><ConnBanner /></div>
{/if}

{#if creating}
  <CreateChannelDialog
    title={creating === 'text' ? 'New chat' : 'New room'}
    confirm="Create"
    onSubmit={create}
    onClose={() => (creating = null)}
  />
{/if}
{#if renaming}
  {@const c = renaming}
  <CreateChannelDialog
    title="Rename"
    initial={c.name}
    onSubmit={async (name) => void (await api.updateChannel(c.id, { name }))}
    onClose={() => (renaming = null)}
  />
{/if}
{#if menu}
  {@const m = menu}
  <ChannelMenu x={m.x} y={m.y} onRename={() => ((renaming = m.c), (menu = null))} onDelete={() => del(m.c)} onClose={() => (menu = null)} />
{/if}
{#if memberMenu}
  {@const mm = memberMenu}
  {#key mm.userId}<UserVolumeMenu userId={mm.userId} x={mm.x} y={mm.y} onClose={() => (memberMenu = null)} />{/key}
{/if}
{#if store.verifying !== null}
  {#key store.verifying}<VerifyDialog userId={store.verifying} onClose={() => (store.verifying = null)} />{/key}
{/if}
{#if settingsOpen && store.me}
  <Settings onClose={() => (settingsOpen = false)} />
{/if}

{#if store.phase === 'app'}<InviteToast onJoin={(c) => { store.view = 'circle'; store.select(c.id); store.joinVoice(c.id); void openPane() }} />{/if}
<DialogHost />

<style>
  .app { height: 100%; display: flex; flex-direction: column; }
  .app { background: var(--desk); }
  .shell { flex: 1; display: grid; grid-template-columns: 272px 1fr; min-height: 0; transition: grid-template-columns 320ms cubic-bezier(0.32, 0.72, 0, 1); }
  /* Tablet, in a room: the sidebar slides away and the room takes the whole width. */
  .shell.tucked { grid-template-columns: 0 1fr; }
  .shell > :global(.side) { overflow: hidden; }
  /* Its contents keep their width while it closes, sliding out rather than squashing. */
  .shell:not(.narrow) > :global(.side > *) { min-width: 248px; }
  .shell.tucked .main { margin-left: 8px; }
  @media (prefers-reduced-motion: reduce) { .shell { transition: none; } }
  /* The page you're on: a sheet on the desk, warmed from the top-left by the lamp. */
  .main {
    position: relative; display: flex; flex-direction: column; min-width: 0; min-height: 0; margin: 8px 8px 8px 0; border-radius: 16px; overflow: hidden;
    background: radial-gradient(120% 70% at 0% 0%, rgba(244, 184, 96, 0.07), transparent 55%), var(--bg);
    box-shadow: 0 0 0 1px var(--line), 0 18px 50px rgba(0, 0, 0, 0.35);
  }
  .none { flex: 1; display: grid; place-items: center; }
  .pane { position: relative; flex: 1; display: flex; flex-direction: column; min-width: 0; min-height: 0; }
  /* Phones: the list fills the screen; an open chat or room slides in over it from the right edge,
     and slides back out on Back while the list eases back in from behind. */
  .shell.narrow { position: relative; display: block; overflow: hidden; }
  .shell.narrow > :global(.side), .shell.narrow .main {
    position: absolute; inset: 0; width: auto;
    transition: transform 360ms cubic-bezier(0.32, 0.72, 0, 1), visibility 0s;
  }
  .mono {
    position: fixed; inset: 0; z-index: 90; pointer-events: none; opacity: 0; visibility: hidden;
    backdrop-filter: grayscale(1) brightness(0.92); -webkit-backdrop-filter: grayscale(1) brightness(0.92);
    transition: opacity 900ms ease-out, visibility 0s 900ms;
  }
  .mono.on { opacity: 1; visibility: visible; transition: opacity 1600ms ease-in-out, visibility 0s; }
  @media (prefers-reduced-motion: reduce) { .mono, .mono.on { transition: none; } }
  .shell.narrow .main { margin: 0; border-radius: 0; z-index: 1; box-shadow: -10px 0 30px rgba(0, 0, 0, 0.35); }
  .shell.narrow:not(.open) .main { transform: translateX(100%); visibility: hidden; transition-delay: 0s, 360ms; }
  .shell.narrow.open > :global(.side) { transform: translateX(-28%); visibility: hidden; transition-delay: 0s, 360ms; }
  /* A room: up from the bottom over a list that stays put and dims a little. */
  .shell.narrow.sheet:not(.open) .main { transform: translateY(100%); }
  .shell.narrow.sheet .main { border-radius: 18px 18px 0 0; box-shadow: 0 -10px 30px rgba(0, 0, 0, 0.35); transition-duration: 420ms; }
  .shell.narrow.sheet.open .main { border-radius: 0; transition: transform 420ms cubic-bezier(0.32, 0.72, 0, 1), border-radius 420ms; }
  .shell.narrow.sheet.open > :global(.side) { transform: scale(0.94); opacity: 0.5; transition: transform 420ms cubic-bezier(0.32, 0.72, 0, 1), opacity 420ms, visibility 0s 420ms; }
  .shell.narrow.sheet > :global(.side) { transition: transform 420ms cubic-bezier(0.32, 0.72, 0, 1), opacity 420ms, visibility 0s; }
  .shell.narrow .main.dragging { transition: none; border-radius: 18px 18px 0 0; }
  /* While a room is pulled down, the list shows behind it. */
  .shell.narrow.sheet.open:has(.main.dragging) > :global(.side) { visibility: visible; transition: none; }
  @media (prefers-reduced-motion: reduce) {
    .shell.narrow > :global(.side), .shell.narrow .main { transition: none; }
  }
</style>
