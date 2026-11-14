<script lang="ts">
  import Avatar from './Avatar.svelte'
  import Icon from './Icon.svelte'
  import { store } from '../lib/state.svelte'
  import { memberGroups } from '../lib/members'
  import { activeNow, presence } from '../lib/home'
  import type { Id, User } from '../lib/types'

  let { onMemberMenu }: { onMemberMenu: (userId: Id, e: MouseEvent) => void } = $props()

  let tab = $state<'online' | 'all'>('online')
  const live = $derived(new Set(store.streams.map((s) => s.userId)))
  const groups = $derived(memberGroups([...store.users.values()].filter((u) => u.id !== store.me?.id), store.online))
  const list = $derived(tab === 'online' ? groups.online : [...groups.online, ...groups.offline])
  const cards = $derived(activeNow(store.voice, store.channels, (id) => store.nick(id), live))

  /** Jump to a voice channel in the circle and join it. */
  function join(channelId: Id) {
    store.view = 'circle'
    store.select(channelId)
    if (store.myVoice.channelId !== channelId) store.joinVoice(channelId)
  }
  const voiceOf = (id: Id) => store.voice.find((v) => v.userId === id)?.channelId ?? null
</script>

<header class="top">
  <span class="ico"><Icon name="users" size={20} /></span>
  <span class="name">Friends</span>
  <span class="sep"></span>
  <button class="tab" class:on={tab === 'online'} onclick={() => (tab = 'online')}>Online</button>
  <button class="tab" class:on={tab === 'all'} onclick={() => (tab = 'all')}>All</button>
</header>

<div class="body">
  <section class="friends">
    <h4>{tab === 'online' ? 'Online' : 'All friends'} — {list.length}</h4>
    {#each list as u (u.id)}
      {@render friend(u)}
    {:else}
      <p class="empty muted">{tab === 'online' ? 'No friends online right now. Check back later!' : 'Nobody else is in the circle yet. Invite your friends from Settings.'}</p>
    {/each}
  </section>

  <aside class="active">
    <h3>Active Now</h3>
    {#each cards as c (c.channelId)}
      <div class="card">
        <div class="stack">
          {#each c.userIds.slice(0, 3) as id (id)}<span class="sa"><Avatar user={store.user(id)} {id} size={30} speaking={store.speaking.has(id)} /></span>{/each}
        </div>
        <div class="ct">
          <span class="title">{c.title}</span>
          <span class="sub">{#if c.live.length}<span class="live">LIVE</span>{/if}<Icon name="volume" size={12} /> {c.channel}</span>
        </div>
        {#if store.myVoice.channelId !== c.channelId}<button class="btn primary join" onclick={() => join(c.channelId)}>Join</button>{:else}<span class="here">You're here</span>{/if}
      </div>
    {:else}
      <div class="quiet">
        <strong>It's quiet for now…</strong>
        <span>When a friend joins a voice channel or goes live, it shows up here.</span>
      </div>
    {/each}
  </aside>
</div>

{#snippet friend(u: User)}
  {@const on = store.online.has(u.id)}
  {@const vc = voiceOf(u.id)}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="row" class:off={!on} oncontextmenu={(e) => { e.preventDefault(); onMemberMenu(u.id, e) }}>
    <span class="av"><Avatar user={u} id={u.id} size={36} speaking={store.speaking.has(u.id)} /><span class="dot" class:on></span></span>
    <span class="txt">
      <span class="nick">{u.nickname}</span>
      <span class="status">{presence(u.id, store.online, store.voice, store.channels, live)}</span>
    </span>
    {#if vc !== null && vc !== store.myVoice.channelId}
      <button class="act" title="Join their voice channel" onclick={() => join(vc)}><Icon name="headphones" size={18} /></button>
    {/if}
  </div>
{/snippet}

<style>
  .top { display: flex; align-items: center; gap: 10px; padding: 10px 18px; border-bottom: 1px solid var(--line); min-height: 49px; }
  .ico { color: var(--faint); display: inline-flex; }
  .name { font-weight: 600; }
  .sep { width: 4px; height: 4px; border-radius: 50%; background: var(--faint); margin: 0 4px; }
  .tab { padding: 3px 10px; border-radius: 6px; color: var(--muted); font-weight: 500; }
  .tab:hover { background: var(--elev-2); color: var(--text); }
  .tab.on { background: var(--elev); color: #fff; }
  .body { flex: 1; display: flex; min-height: 0; }
  .friends { flex: 1; min-width: 0; overflow-y: auto; padding: 14px 20px; }
  h4 { margin: 4px 0 8px; font-size: 11.5px; font-weight: 700; letter-spacing: 0.04em; text-transform: uppercase; color: var(--muted); }
  .row { display: flex; align-items: center; gap: 12px; padding: 9px 10px; border-top: 1px solid var(--line); border-radius: 8px; }
  .row:hover { background: var(--elev-2); border-top-color: transparent; }
  .row:hover + .row { border-top-color: transparent; }
  .row.off { opacity: 0.5; }
  .av { position: relative; display: inline-flex; }
  .dot { position: absolute; right: -2px; bottom: -2px; z-index: 2; width: 13px; height: 13px; border-radius: 50%; background: var(--faint); border: 3px solid var(--bg); box-sizing: border-box; }
  .row:hover .dot { border-color: var(--elev-2); }
  .dot.on { background: var(--speaking); }
  .txt { flex: 1; min-width: 0; display: flex; flex-direction: column; }
  .nick { font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .status { font-size: 12.5px; color: var(--muted); }
  .act { width: 36px; height: 36px; border-radius: 50%; background: var(--panel); color: var(--muted); display: grid; place-items: center; }
  .act:hover { color: var(--text); background: var(--elev); }
  .empty { text-align: center; margin-top: 18vh; }

  .active { width: 300px; flex-shrink: 0; overflow-y: auto; border-left: 1px solid var(--line); padding: 16px; }
  h3 { margin: 0 0 12px; font-size: 18px; font-weight: 700; }
  .card { display: flex; align-items: center; gap: 10px; padding: 12px; background: var(--panel); border: 1px solid var(--line); border-radius: 10px; margin-bottom: 8px; animation: card-in 220ms cubic-bezier(0.2, 0.8, 0.2, 1); }
  @keyframes card-in { from { opacity: 0; transform: translateY(6px); } }
  .stack { display: flex; }
  .sa + .sa { margin-left: -10px; }
  .sa { display: inline-flex; border-radius: 50%; box-shadow: 0 0 0 2px var(--panel); }
  .ct { flex: 1; min-width: 0; display: flex; flex-direction: column; }
  .title { font-weight: 600; font-size: 14px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .sub { display: flex; align-items: center; gap: 4px; font-size: 12px; color: var(--muted); }
  .live { background: var(--danger); color: #fff; font-size: 10px; font-weight: 700; padding: 0 5px; border-radius: 3px; margin-right: 2px; }
  .join { padding: 5px 12px; font-size: 13px; }
  .here { font-size: 12px; color: var(--speaking); white-space: nowrap; }
  .quiet { text-align: center; padding: 22px 12px; display: flex; flex-direction: column; gap: 6px; color: var(--muted); font-size: 13px; }
  .quiet strong { color: var(--text); font-size: 15px; }
  @media (prefers-reduced-motion: reduce) { .card { animation: none; } }
</style>
