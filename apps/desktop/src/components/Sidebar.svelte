<script lang="ts">
  import { store } from '../lib/state.svelte'
  import { memberGroups } from '../lib/members'
  import type { Channel, Id, User } from '../lib/types'
  import Avatar from './Avatar.svelte'
  import Icon from './Icon.svelte'
  import VoiceMembers from './VoiceMembers.svelte'
  import CallTimer from './CallTimer.svelte'
  import UserPanel from './UserPanel.svelte'

  // The one panel on the left: who's around, the chats, the rooms, and me.
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
  const home = $derived(store.view === 'home')
  // Everyone in the circle, the ones around first.
  const people = $derived.by(() => {
    const g = memberGroups(store.people(), store.online, me.id)
    return [...g.online.map((u) => ({ u, on: true })), ...g.offline.map((u) => ({ u, on: false }))]
  })
  const inVoice = (id: Id) => store.voice.some((v) => v.userId === id)

  function menu(c: Channel, e: MouseEvent) {
    if (!canManage(c) || !onChannelMenu) return
    e.preventDefault()
    onChannelMenu(c, e)
  }
</script>

<aside class="side">
  <header class="brand">
    <span class="mark"><Icon name="logo" size={16} /></span>
    <span class="word">{circleName}</span>
  </header>

  <div class="switch" role="tablist" aria-label="Show">
    <button role="tab" aria-selected={!home} class:on={!home} onclick={() => (store.view = 'circle')}>Rooms</button>
    <button role="tab" aria-selected={home} class:on={home} onclick={() => (store.view = 'home')}>Friends</button>
  </div>

  <!-- Who's around: with four friends, faces say more than a list. -->
  <div class="crew" aria-label="People">
    {#each people as { u, on } (u.id)}
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="face" class:off={!on} title="{u.nickname}{on ? '' : ' (offline)'}" oncontextmenu={(e) => { e.preventDefault(); if (u.id !== me.id) onMemberMenu(u.id, e) }}>
        <span class="av">
          <Avatar user={u} id={u.id} size={40} speaking={store.speaking.has(u.id)} />
          {#if on}<span class="dot" class:voice={inVoice(u.id)}></span>{/if}
        </span>
        <span class="nm">{u.id === me.id ? 'You' : u.nickname}</span>
      </div>
    {/each}
  </div>

  <nav class="lists">
    {#each [{ label: 'Chats', kind: 'text' as const, list: text, what: 'chat' }, { label: 'Rooms', kind: 'voice' as const, list: voice, what: 'room' }] as g (g.kind)}
      <div class="grp">
        <span>{g.label}</span>
        <button class="add" title="New {g.what}" aria-label="New {g.what}" onclick={() => onCreate(g.kind)}><Icon name="plus" size={14} /></button>
      </div>
      {#each g.list as c (c.id)}
        <button class="ch" class:voice={c.kind === 'voice'} class:on={!home && c.id === selectedId} onclick={() => onSelect(c)} oncontextmenu={(e) => menu(c, e)}>
          <span class="ico"><Icon name={c.kind === 'text' ? 'hash' : 'volume'} size={15} /></span>
          <span class="name">{c.name}</span>
          {#if c.kind === 'voice'}<span class="len" title="How long this call has been going"><CallTimer channelId={c.id} /></span>{/if}
        </button>
        {#if c.kind === 'voice'}<div class="occ"><VoiceMembers channelId={c.id} onMenu={onMemberMenu} /></div>{/if}
      {/each}
    {/each}
  </nav>

  <UserPanel {me} {onSettings} />
</aside>

<style>
  .side { background: var(--desk); display: flex; flex-direction: column; min-height: 0; min-width: 0; }
  .brand { display: flex; align-items: center; gap: 8px; padding: 16px 18px 12px; }
  .mark { color: var(--accent); display: inline-flex; }
  .word { font-size: 19px; font-weight: 750; font-stretch: 80%; letter-spacing: -0.01em; font-variation-settings: 'opsz' 36; }

  .switch { display: grid; grid-template-columns: 1fr 1fr; margin: 0 14px; padding: 3px; border-radius: 12px; background: var(--panel-2); border: 1px solid var(--line); }
  .switch button { padding: 6px 0; border-radius: 9px; font-weight: 600; font-size: 13.5px; color: var(--muted); transition: background 160ms, color 160ms; }
  .switch button:hover { color: var(--text); }
  .switch button.on { background: var(--elev); color: var(--text); box-shadow: inset 0 -2px 0 var(--accent); }

  .crew { display: flex; gap: 6px; padding: 14px 12px 6px; overflow-x: auto; scrollbar-width: none; }
  .crew::-webkit-scrollbar { display: none; }
  .face { flex: none; width: 56px; display: flex; flex-direction: column; align-items: center; gap: 5px; cursor: default; }
  .face.off { opacity: 0.42; }
  .av { position: relative; display: inline-flex; }
  .dot { position: absolute; right: -1px; bottom: -1px; z-index: 2; width: 13px; height: 13px; border-radius: 50%; background: var(--speaking); border: 3px solid var(--desk); box-sizing: border-box; }
  .dot.voice { background: var(--accent); }
  .nm { max-width: 100%; font-size: 11.5px; font-weight: 600; color: var(--muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

  .lists { flex: 1; overflow-y: auto; padding: 4px 10px 10px; }
  .grp { display: flex; justify-content: space-between; align-items: center; margin: 16px 8px 4px; font-size: 13px; color: var(--muted); font-weight: 650; }
  .add { display: grid; place-items: center; width: 22px; height: 22px; color: var(--muted); border-radius: 7px; }
  .add:hover { color: var(--text); background: var(--elev-2); }
  .ch {
    position: relative; display: flex; align-items: center; gap: 9px; width: 100%; text-align: left; padding: 7px 10px; border-radius: 10px;
    color: var(--muted); font-weight: 500; transition: background 140ms, color 140ms;
  }
  .ch:hover { background: var(--elev-2); color: var(--text); }
  /* Where you are: lit by the lamp. */
  .ch.on { background: var(--accent-soft); color: var(--accent); font-weight: 650; }
  .ch.on .ico { opacity: 1; }
  .ico { width: 16px; display: inline-flex; opacity: 0.7; }
  .len { margin-left: auto; flex: none; font-size: 12px; color: var(--faint); font-variant-numeric: tabular-nums; }
  .name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .occ { padding-left: 6px; }
  @media (prefers-reduced-motion: reduce) { .switch button, .ch { transition: none; } }
</style>
