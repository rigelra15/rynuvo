<script lang="ts">
  import { ago } from '../lib/ago'
  import { clock } from '../lib/clock.svelte'
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
  // Everyone else in the circle, the ones around first (I'm in the profile bar at the bottom).
  const people = $derived.by(() => {
    const g = memberGroups(store.people(), store.online, me.id)
    return [...g.online.map((u) => ({ u, on: true })), ...g.offline.map((u) => ({ u, on: false }))].filter(({ u }) => u.id !== me.id)
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
  {#if people.length}
  <div class="crew" aria-label="People">
    {#each people as { u, on } (u.id)}
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="face" class:off={!on} title="{u.nickname}{on ? '' : ' (offline)'}" oncontextmenu={(e) => { e.preventDefault(); onMemberMenu(u.id, e) }}>
        <span class="av">
          <Avatar user={u} id={u.id} size={40} speaking={store.speaking.has(u.id)} />
          {#if on}<span class="dot" class:voice={inVoice(u.id)} class:mobile={store.mobile.has(u.id)}></span>
          {:else if store.lastSeen.has(u.id)}<span class="seen">{ago(store.lastSeen.get(u.id)!, clock.now)}</span>{/if}
        </span>
        <span class="nm">{u.nickname}</span>
      </div>
    {/each}
  </div>
  {/if}

  <nav class="lists">
    {#each [{ label: 'Chats', kind: 'text' as const, list: text, what: 'chat' }, { label: 'Rooms', kind: 'voice' as const, list: voice, what: 'room' }] as g (g.kind)}
      <div class="grp">
        <span>{g.label}</span>
        <button class="add" title="New {g.what}" aria-label="New {g.what}" onclick={() => onCreate(g.kind)}><Icon name="plus" size={14} /></button>
      </div>
      {#each g.list as c (c.id)}
        {@const unread = c.kind === 'text' && store.isUnread(c.id)}
        {@const pinged = unread && store.isMentioned(c.id)}
        <button class="ch" class:voice={c.kind === 'voice'} class:on={!home && c.id === selectedId} class:unread class:pinged onclick={() => onSelect(c)} oncontextmenu={(e) => menu(c, e)}
          aria-label={pinged ? `${c.name}, you were mentioned` : unread ? `${c.name}, unread messages` : undefined}>
          <span class="ico"><Icon name={c.kind === 'text' ? 'hash' : 'volume'} size={15} /></span>
          <span class="name">{c.name}</span>
          {#if pinged}<span class="at" aria-hidden="true">@</span>{:else if unread}<span class="new" aria-hidden="true"></span>{/if}
          {#if c.kind === 'voice'}<span class="len" title="How long this call has been going"><CallTimer channelId={c.id} /></span>{/if}
        </button>
        {#if c.kind === 'voice'}<div class="occ"><VoiceMembers channelId={c.id} onMenu={onMemberMenu} /></div>{/if}
      {/each}
    {/each}
  </nav>

  <UserPanel {me} {onSettings} onOpenCall={onSelect} />
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
  /* On a phone only: the dot becomes a little phone (a screen over a lit chin). */
  .dot.mobile { width: 12px; height: 17px; border-radius: 4px; }
  .dot.mobile::after { content: ''; position: absolute; left: 1px; right: 1px; top: 1px; bottom: 3px; border-radius: 1px; background: var(--desk); }
  /* Offline friends: how long since they were last here. */
  .seen {
    position: absolute; right: -6px; bottom: -4px; z-index: 2; height: 15px; padding: 0 4px; border-radius: 999px; display: grid; place-items: center;
    font-size: 9.5px; font-weight: 750; line-height: 1; color: var(--muted); background: var(--elev); box-shadow: 0 0 0 2px var(--desk); font-variant-numeric: tabular-nums;
  }
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
  /* Something new: the name lights up, with a soft glow and a dot. */
  .ch.unread { color: var(--text); font-weight: 650; background: linear-gradient(90deg, rgba(196, 181, 255, 0.1), transparent 75%); }
  .ch.unread .ico { opacity: 1; }
  .new {
    margin-left: auto; flex: none; width: 8px; height: 8px; border-radius: 50%; background: var(--lilac, #c4b5ff);
    box-shadow: 0 0 0 3px rgba(196, 181, 255, 0.14), 0 0 10px rgba(196, 181, 255, 0.7); animation: pop 360ms cubic-bezier(0.34, 1.56, 0.64, 1);
  }
  /* Someone called me (or @everyone): the lamp's gold, and an @ that glows. */
  .ch.pinged { background: linear-gradient(90deg, rgba(244, 184, 96, 0.16), rgba(244, 184, 96, 0.03) 80%); box-shadow: inset 0 0 0 1px rgba(244, 184, 96, 0.22); }
  .at {
    margin-left: auto; flex: none; min-width: 20px; height: 20px; padding: 0 6px; border-radius: 10px; display: grid; place-items: center;
    font-size: 12px; font-weight: 800; color: #2a1a05; background: linear-gradient(135deg, #ffd690, #f4b860);
    box-shadow: 0 0 12px rgba(244, 184, 96, 0.65); animation: pop 360ms cubic-bezier(0.34, 1.56, 0.64, 1), glow 2.4s 400ms ease-in-out infinite;
  }
  .ch.on.unread, .ch.on.pinged { background: var(--accent-soft); box-shadow: none; }
  @keyframes pop { from { transform: scale(0); } }
  @keyframes glow { 50% { box-shadow: 0 0 4px rgba(244, 184, 96, 0.35); } }
  .len { margin-left: auto; flex: none; font-size: 12px; color: var(--faint); font-variant-numeric: tabular-nums; }
  .name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .occ { padding-left: 6px; }
  @media (prefers-reduced-motion: reduce) { .switch button, .ch { transition: none; } .new, .at { animation: none; } }
  /* Fingers, not a pointer: taller rows and a bigger + to hit. */
  @media (pointer: coarse) {
    .ch { padding: 11px 10px; }
    .switch button { padding: 9px 0; }
    .add { width: 34px; height: 34px; margin-right: -6px; }
  }
</style>
