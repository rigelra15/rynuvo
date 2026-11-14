<script lang="ts">
  import { store } from '../lib/state.svelte'
  import type { Channel, Id, User } from '../lib/types'
  import Icon from './Icon.svelte'
  import VoiceMembers from './VoiceMembers.svelte'
  import UserPanel from './UserPanel.svelte'

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
    onMemberMenu,
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
    onMemberMenu: (userId: Id, e: MouseEvent) => void
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
  <header class="brand"><span class="mark"><Icon name="logo" size={14} /></span> {circleName}</header>

  <nav class="lists">
    {#each [{ label: 'Text', kind: 'text' as const, list: text }, { label: 'Voice', kind: 'voice' as const, list: voice }] as g}
      <div class="grp">
        <span>{g.label}</span>
        <button class="add" title="Create {g.label.toLowerCase()} channel" onclick={() => onCreate(g.kind)}><Icon name="plus" size={14} /></button>
      </div>
      {#each g.list as c (c.id)}
        <button
          class="ch"
          class:on={c.id === selectedId}
          onclick={() => onSelect(c)}
          oncontextmenu={(e) => menu(c, e)}
        >
          <span class="ico"><Icon name={c.kind === 'text' ? 'hash' : 'volume'} size={16} /></span>
          <span class="name">{c.name}</span>
        </button>
        {#if c.kind === 'voice'}<VoiceMembers channelId={c.id} onMenu={onMemberMenu} />{/if}
      {/each}
    {/each}
  </nav>

  <UserPanel {me} {onSettings} />
</aside>

<style>
  .side { background: var(--panel); display: flex; flex-direction: column; min-height: 0; }
  .brand { font-weight: 700; padding: 16px 16px 10px; color: #fff; display: flex; gap: 8px; align-items: center; }
  .mark { color: var(--accent); display: inline-flex; }
  .lists { flex: 1; overflow-y: auto; padding: 0 8px 8px; }
  .grp { display: flex; justify-content: space-between; align-items: center; margin: 14px 8px 4px; font-size: 11px; letter-spacing: 0.06em; text-transform: uppercase; color: var(--muted); font-weight: 600; }
  .add { display: grid; place-items: center; color: var(--muted); padding: 2px; border-radius: 4px; }
  .add:hover { color: var(--text); }
  .ch { display: flex; gap: 8px; width: 100%; text-align: left; padding: 6px 8px; border-radius: 6px; color: var(--muted); }
  .ch:hover { background: var(--elev-2); color: var(--text); }
  .ch.on { background: var(--elev); color: #fff; }
  .ico { width: 16px; display: inline-flex; opacity: 0.8; }
  .name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
</style>
