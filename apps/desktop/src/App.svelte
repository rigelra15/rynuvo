<script lang="ts">
  import { onMount } from 'svelte'
  import ChannelMenu from './components/ChannelMenu.svelte'
  import ChannelView from './components/ChannelView.svelte'
  import ConnBanner from './components/ConnBanner.svelte'
  import CreateChannelDialog from './components/CreateChannelDialog.svelte'
  import Onboarding from './components/Onboarding.svelte'
  import Settings from './components/Settings.svelte'
  import DialogHost from './components/DialogHost.svelte'
  import { dialog } from './lib/dialog.svelte'
  import Sidebar from './components/Sidebar.svelte'
  import UserVolumeMenu from './components/UserVolumeMenu.svelte'
  import { api, errText, onSessionEvent, onStreamEvent, onVoiceEvent } from './lib/api'
  import { store } from './lib/state.svelte'
  import type { Channel, ChannelKind, Id } from './lib/types'

  let creating = $state<ChannelKind | null>(null)
  let renaming = $state<Channel | null>(null)
  let menu = $state<{ c: Channel; x: number; y: number } | null>(null)
  let settingsOpen = $state(false)
  let memberMenu = $state<{ userId: Id; x: number; y: number } | null>(null)

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

  onMount(() => {
    const un = onSessionEvent((e) => store.apply(e))
    const unv = onVoiceEvent((e) => store.applyVoiceEvent(e))
    const uns = onStreamEvent((e) => store.applyStreamEvent(e))
    void store.boot()
    return () => {
      void un.then((f) => f())
      void unv.then((f) => f())
      void uns.then((f) => f())
    }
  })

  async function create(name: string) {
    const c = await api.createChannel(creating!, name)
    store.apply({ kind: 'server', data: { t: 'channelCreated', d: c } })
    store.select(c.id)
  }

  async function del(c: Channel) {
    menu = null
    if (!(await dialog.ask(`Delete channel "${c.name}" and all of its messages?`, 'Delete'))) return
    try {
      await api.deleteChannel(c.id)
    } catch (e) {
      void dialog.notice(errText(e))
    }
  }
</script>

{#if store.phase === 'boot'}
  <div class="boot">◆</div>
{:else if store.phase === 'onboarding'}
  <Onboarding />
{:else if store.me}
  <div class="app">
    <ConnBanner />
    <div class="shell">
      <Sidebar
        channels={store.sortedChannels()}
        selectedId={store.selectedId}
        me={store.me}
        canManage={store.canManage}
        onSelect={(c) => {
          store.select(c.id)
          if (c.kind === 'voice') store.joinVoice(c.id)
        }}
        onMemberMenu={(userId, e) => (memberMenu = { userId, x: e.clientX, y: e.clientY })}
        onCreate={(k) => (creating = k)}
        onChannelMenu={(c, e) => (menu = { c, x: e.clientX, y: e.clientY })}
        onSettings={() => (settingsOpen = true)}
      />
      <main class="main">
        {#if store.selected}
          {#key store.selected.id}
            <ChannelView channel={store.selected} onMemberMenu={(userId, e) => (memberMenu = { userId, x: e.clientX, y: e.clientY })} />
          {/key}
        {:else}
          <div class="none muted">No channels yet. Click + next to "Text" to create one.</div>
        {/if}
      </main>
    </div>
  </div>
{:else}
  <div class="app"><ConnBanner /><div class="boot">◆</div></div>
{/if}

{#if creating}
  <CreateChannelDialog
    title={creating === 'text' ? 'Create text channel' : 'Create voice channel'}
    confirm="Create"
    onSubmit={create}
    onClose={() => (creating = null)}
  />
{/if}
{#if renaming}
  {@const c = renaming}
  <CreateChannelDialog
    title="Rename channel"
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
{#if settingsOpen && store.me}
  <Settings onClose={() => (settingsOpen = false)} />
{/if}

<DialogHost />

<style>
  .app { height: 100%; display: flex; flex-direction: column; }
  .shell { flex: 1; display: grid; grid-template-columns: 240px 1fr; min-height: 0; }
  .main { display: flex; flex-direction: column; min-width: 0; min-height: 0; background: var(--bg); }
  .none { flex: 1; display: grid; place-items: center; }
  .boot { height: 100%; display: grid; place-items: center; color: var(--accent); font-size: 40px; }
</style>
