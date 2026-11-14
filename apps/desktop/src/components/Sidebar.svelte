<script lang="ts">
  import type { Channel, User } from '../lib/types'

  let {
    circleName = 'Rynuvo',
    channels,
    selectedId,
    me,
    canManage = () => false,
    onSelect,
    onCreate,
    onChannelMenu,
    onSettings,
  }: {
    circleName?: string
    channels: Channel[]
    selectedId: number | null
    me: User
    canManage?: (c: Channel) => boolean
    onSelect: (c: Channel) => void
    onCreate: (kind: 'text' | 'voice') => void
    onChannelMenu?: (c: Channel, e: MouseEvent) => void
    onSettings: () => void
  } = $props()

  const text = $derived(channels.filter((c) => c.kind === 'text'))
  const voice = $derived(channels.filter((c) => c.kind === 'voice'))

  function menu(c: Channel, e: MouseEvent) {
    if (!canManage(c) || !onChannelMenu) return
    e.preventDefault()
    onChannelMenu(c, e)
  }
</script>

<aside class="side">
  <header class="brand"><span class="mark">◆</span> {circleName}</header>

  <nav class="lists">
    {#each [{ label: 'Text', kind: 'text' as const, list: text }, { label: 'Voice', kind: 'voice' as const, list: voice }] as g}
      <div class="grp">
        <span>{g.label}</span>
        <button class="add" title="Create {g.label.toLowerCase()} channel" onclick={() => onCreate(g.kind)}>+</button>
      </div>
      {#each g.list as c (c.id)}
        <button
          class="ch"
          class:on={c.id === selectedId}
          onclick={() => onSelect(c)}
          oncontextmenu={(e) => menu(c, e)}
        >
          <span class="ico">{c.kind === 'text' ? '#' : '🔊'}</span>
          <span class="name">{c.name}</span>
        </button>
      {/each}
    {/each}
  </nav>

  <footer class="me">
    <span class="av">{me.nickname.slice(0, 1).toUpperCase()}</span>
    <span class="nick">{me.nickname}</span>
    <button class="gear" title="Settings" onclick={onSettings}>⚙</button>
  </footer>
</aside>

<style>
  .side { background: var(--panel); display: flex; flex-direction: column; min-height: 0; }
  .brand { font-weight: 700; padding: 16px 16px 10px; color: #fff; display: flex; gap: 8px; align-items: center; }
  .mark { color: var(--accent); }
  .lists { flex: 1; overflow-y: auto; padding: 0 8px 8px; }
  .grp { display: flex; justify-content: space-between; align-items: center; margin: 14px 8px 4px; font-size: 11px; letter-spacing: 0.06em; text-transform: uppercase; color: var(--muted); font-weight: 600; }
  .add { font-size: 16px; line-height: 1; color: var(--muted); padding: 0 4px; }
  .add:hover { color: var(--text); }
  .ch { display: flex; gap: 8px; width: 100%; text-align: left; padding: 6px 8px; border-radius: 6px; color: var(--muted); }
  .ch:hover { background: var(--elev-2); color: var(--text); }
  .ch.on { background: var(--elev); color: #fff; }
  .ico { width: 16px; text-align: center; opacity: 0.8; }
  .name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .me { display: flex; align-items: center; gap: 8px; padding: 10px 12px; background: var(--panel-2); }
  .av { width: 28px; height: 28px; border-radius: 50%; background: var(--accent); color: #fff; display: grid; place-items: center; font-weight: 700; font-size: 13px; }
  .nick { flex: 1; font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .gear { color: var(--muted); font-size: 16px; padding: 4px; border-radius: 6px; }
  .gear:hover { color: var(--text); background: var(--elev); }
</style>
