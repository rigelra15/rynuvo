<script lang="ts">
  import { onMount } from 'svelte'
  import ChannelMenu from './components/ChannelMenu.svelte'
  import ChannelView from './components/ChannelView.svelte'
  import ConnBanner from './components/ConnBanner.svelte'
  import CreateChannelDialog from './components/CreateChannelDialog.svelte'
  import Onboarding from './components/Onboarding.svelte'
  import Sidebar from './components/Sidebar.svelte'
  import { api, errText, onSessionEvent } from './lib/api'
  import { store } from './lib/state.svelte'
  import type { Channel, ChannelKind } from './lib/types'

  let creating = $state<ChannelKind | null>(null)
  let renaming = $state<Channel | null>(null)
  let menu = $state<{ c: Channel; x: number; y: number } | null>(null)
  let settingsOpen = $state(false)

  onMount(() => {
    const un = onSessionEvent((e) => store.apply(e))
    void store.boot()
    return () => void un.then((f) => f())
  })

  async function create(name: string) {
    const c = await api.createChannel(creating!, name)
    store.apply({ kind: 'server', data: { t: 'channelCreated', d: c } })
    store.select(c.id)
  }

  async function del(c: Channel) {
    menu = null
    if (!confirm(`Hapus channel "${c.name}" beserta semua pesannya?`)) return
    try {
      await api.deleteChannel(c.id)
    } catch (e) {
      alert(errText(e))
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
        onSelect={(c) => store.select(c.id)}
        onCreate={(k) => (creating = k)}
        onChannelMenu={(c, e) => (menu = { c, x: e.clientX, y: e.clientY })}
        onSettings={() => (settingsOpen = true)}
      />
      <main class="main">
        {#if store.selected}
          <ChannelView channel={store.selected} />
        {:else}
          <div class="none muted">Belum ada channel. Klik + di samping "Text" untuk membuat.</div>
        {/if}
      </main>
    </div>
  </div>
{:else}
  <div class="app"><ConnBanner /><div class="boot">◆</div></div>
{/if}

{#if creating}
  <CreateChannelDialog
    title={creating === 'text' ? 'Buat text channel' : 'Buat voice channel'}
    confirm="Buat"
    onSubmit={create}
    onClose={() => (creating = null)}
  />
{/if}
{#if renaming}
  {@const c = renaming}
  <CreateChannelDialog
    title="Ganti nama channel"
    initial={c.name}
    onSubmit={async (name) => void (await api.updateChannel(c.id, { name }))}
    onClose={() => (renaming = null)}
  />
{/if}
{#if menu}
  {@const m = menu}
  <ChannelMenu x={m.x} y={m.y} onRename={() => ((renaming = m.c), (menu = null))} onDelete={() => del(m.c)} onClose={() => (menu = null)} />
{/if}
{#if settingsOpen}
  <div class="tmp-settings">
    <button class="btn" onclick={() => (settingsOpen = false)}>Tutup</button>
    <button class="btn danger" onclick={() => store.logout()}>Keluar dari perangkat ini</button>
  </div>
{/if}

<style>
  .app { height: 100%; display: flex; flex-direction: column; }
  .shell { flex: 1; display: grid; grid-template-columns: 240px 1fr; min-height: 0; }
  .main { display: flex; flex-direction: column; min-width: 0; min-height: 0; background: var(--bg); }
  .none { flex: 1; display: grid; place-items: center; }
  .boot { height: 100%; display: grid; place-items: center; color: var(--accent); font-size: 40px; }
  .tmp-settings { position: fixed; right: 16px; bottom: 16px; display: flex; gap: 8px; }
</style>
