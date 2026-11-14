<script lang="ts">
  import Sidebar from './components/Sidebar.svelte'
  import type { Channel, User } from './lib/types'

  // Static preview data (replaced by live state in Task 12).
  const me: User = { id: 1, nickname: 'kamu', isAdmin: true }
  const channels: Channel[] = [
    { id: 1, kind: 'text', name: 'umum', topic: null, ownerId: 1, position: 1 },
    { id: 2, kind: 'text', name: 'clip-game', topic: null, ownerId: 1, position: 2 },
    { id: 3, kind: 'voice', name: 'Mabar', topic: null, ownerId: 1, position: 1 },
  ]
  let selected = $state(1)
</script>

<div class="shell">
  <Sidebar {channels} selectedId={selected} {me} onSelect={(c) => (selected = c.id)} onCreate={() => {}} onSettings={() => {}} />
  <main class="main">
    <header class="top"># {channels.find((c) => c.id === selected)?.name}</header>
    <section class="empty muted">Belum ada pesan. Sapa teman-temanmu!</section>
  </main>
</div>

<style>
  .shell { display: grid; grid-template-columns: 240px 1fr; height: 100%; }
  .main { display: flex; flex-direction: column; min-width: 0; background: var(--bg); }
  .top { padding: 14px 18px; font-weight: 600; border-bottom: 1px solid var(--line); }
  .empty { flex: 1; display: grid; place-items: center; }
</style>
