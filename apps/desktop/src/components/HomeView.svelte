<script lang="ts">
  import { pane } from '../lib/pane.svelte'
  import { ago } from '../lib/ago'
  import { clock } from '../lib/clock.svelte'
  import Avatar from './Avatar.svelte'
  import Icon from './Icon.svelte'
  import { store } from '../lib/state.svelte'
  import { memberGroups } from '../lib/members'
  import { activeNow, presence } from '../lib/home'
  import type { Id, User } from '../lib/types'

  let { onMemberMenu }: { onMemberMenu: (userId: Id, e: MouseEvent) => void } = $props()

  const live = $derived(new Set(store.streams.map((s) => s.userId)))
  const groups = $derived(memberGroups(store.people().filter((u) => u.id !== store.me?.id), store.online))
  // Everyone, the ones around first.
  const list = $derived([...groups.online, ...groups.offline])
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
  {#if pane.narrow}<button class="away" title="Back to chats & rooms" aria-label="Back to chats & rooms" onclick={pane.close}><Icon name="back" size={22} /></button>{/if}
  <h1 class="title-xl">Friends</h1>
</header>

<div class="body">
  <section class="friends">
    {#each list as u (u.id)}
      {@render friend(u)}
    {:else}
      <p class="empty muted">Nobody else is here yet. Invite your friends from Settings.</p>
    {/each}
  </section>

  <aside class="active">
    <h3>Happening now</h3>
    {#each cards as c (c.channelId)}
      <div class="card">
        <div class="stack">
          {#each c.userIds.slice(0, 3) as id (id)}<span class="sa"><Avatar user={store.user(id)} {id} size={30} speaking={store.speaking.has(id)} /></span>{/each}
        </div>
        <div class="ct">
          <span class="title">{c.title}</span>
          <span class="sub">{#if c.live.length}<span class="live">Live</span>{/if}<Icon name="volume" size={12} /> {c.channel}</span>
        </div>
        {#if store.myVoice.channelId !== c.channelId}<button class="btn primary join" onclick={() => join(c.channelId)}>Join</button>{:else}<span class="here">You're here</span>{/if}
      </div>
    {:else}
      <div class="quiet">
        <strong>Nobody's in a room</strong>
        <span>When a friend joins a room or goes live, you'll see it here.</span>
      </div>
    {/each}
  </aside>
</div>

{#snippet friend(u: User)}
  {@const on = store.online.has(u.id)}
  {@const vc = voiceOf(u.id)}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="row" class:off={!on} oncontextmenu={(e) => { e.preventDefault(); onMemberMenu(u.id, e) }}>
    <span class="av"><Avatar user={u} id={u.id} size={36} speaking={store.speaking.has(u.id)} />{#if !on && store.lastSeen.has(u.id)}<span class="seen">{ago(store.lastSeen.get(u.id)!, clock.now)}</span>{:else}<span class="dot" class:on class:mobile={on && store.mobile.has(u.id)}></span>{/if}</span>
    <span class="txt">
      <span class="nick">{u.nickname}</span>
      <span class="status">{#if store.games.has(u.id)}<Icon name="gamepad" size={13} /> {/if}{presence(u.id, store.online, store.voice, store.channels, live, store.games.get(u.id))}</span>
    </span>
    {#if vc !== null && vc !== store.myVoice.channelId}
      <button class="act" title="Join their voice channel" onclick={() => join(vc)}><Icon name="headphones" size={18} /></button>
    {/if}
  </div>
{/snippet}

<style>
  .away { flex: none; width: 36px; height: 36px; margin: 0 -4px 0 -10px; border-radius: 10px; display: grid; place-items: center; color: var(--text); }
  .top { display: flex; align-items: center; gap: 16px; padding: 18px 22px 12px; min-height: 64px; }
  h1 { margin: 0; }
  .body { flex: 1; display: flex; min-height: 0; }
  .friends { flex: 1; min-width: 0; overflow-y: auto; padding: 8px 22px 16px; }
  .row { display: flex; align-items: center; gap: 14px; padding: 10px 12px; border-radius: 14px; transition: background 140ms; }
  .row:hover { background: var(--elev-2); }
  .row.off { opacity: 0.5; }
  .av { position: relative; display: inline-flex; }
  .dot { position: absolute; right: -2px; bottom: -2px; z-index: 2; width: 13px; height: 13px; border-radius: 50%; background: var(--faint); border: 3px solid var(--bg); box-sizing: border-box; }
  .row:hover .dot { border-color: var(--elev-2); }
  .dot.on { background: var(--speaking); }
  /* On a phone only: the dot becomes a little phone (a screen over a lit chin). */
  .dot.mobile { width: 12px; height: 17px; border-radius: 4px; }
  .dot.mobile::after { content: ''; position: absolute; left: 1px; right: 1px; top: 1px; bottom: 3px; border-radius: 1px; background: var(--bg); }
  /* Offline friends: how long since they were last here. */
  .seen {
    position: absolute; right: -6px; bottom: -4px; z-index: 2; height: 15px; padding: 0 4px; border-radius: 999px; display: grid; place-items: center;
    font-size: 9.5px; font-weight: 750; line-height: 1; color: var(--muted); background: var(--elev); box-shadow: 0 0 0 2px var(--bg); font-variant-numeric: tabular-nums;
  }
  .txt { flex: 1; min-width: 0; display: flex; flex-direction: column; }
  .nick { font-weight: 700; font-size: 15px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .status { display: flex; align-items: center; gap: 4px; font-size: 12.5px; color: var(--muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .status :global(svg) { flex: none; color: var(--speaking); }
  .act { width: 38px; height: 38px; border-radius: 12px; background: var(--accent-soft); color: var(--accent); display: grid; place-items: center; }
  .act:hover { background: var(--accent); color: var(--on-accent); }
  .empty { text-align: center; margin-top: 18vh; }

  .active { width: 300px; flex-shrink: 0; overflow-y: auto; padding: 8px 18px 18px 6px; }
  h3 { margin: 0 0 12px; font-size: 15px; font-weight: 700; color: var(--muted); }
  .card { display: flex; align-items: center; gap: 10px; padding: 12px; background: var(--panel); border: 1px solid var(--line); border-radius: 16px; margin-bottom: 8px; animation: card-in 220ms cubic-bezier(0.2, 0.8, 0.2, 1); }
  @keyframes card-in { from { opacity: 0; transform: translateY(6px); } }
  .stack { display: flex; }
  .sa + .sa { margin-left: -10px; }
  .sa { display: inline-flex; border-radius: 50%; box-shadow: 0 0 0 2px var(--panel); }
  .ct { flex: 1; min-width: 0; display: flex; flex-direction: column; }
  .title { font-weight: 700; font-size: 14px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .sub { display: flex; align-items: center; gap: 4px; font-size: 12px; color: var(--muted); }
  .live { background: var(--danger); color: #fff; font-size: 10.5px; font-weight: 700; padding: 0 6px; border-radius: 5px; margin-right: 2px; }
  .join { padding: 5px 12px; font-size: 13px; }
  .here { font-size: 12px; color: var(--accent); white-space: nowrap; font-weight: 600; }
  .quiet { padding: 18px 16px; display: flex; flex-direction: column; gap: 6px; color: var(--muted); font-size: 13px; border: 1px dashed var(--line); border-radius: 16px; }
  .quiet strong { color: var(--text); font-size: 15px; }
  /* Phones: what's happening now on top, then everyone, in one scroll. */
  @media (max-width: 760px) {
    .top { padding: 12px 16px 8px; min-height: 0; }
    .body { flex-direction: column-reverse; justify-content: flex-end; overflow-y: auto; }
    .friends { flex: none; overflow: visible; padding: 4px 16px 16px; }
    .active { width: auto; overflow: visible; padding: 8px 16px 0; }
  }
  @media (prefers-reduced-motion: reduce) { .card { animation: none; } .row { transition: none; } }
</style>
