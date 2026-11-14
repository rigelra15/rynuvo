<script lang="ts">
  import Avatar from './Avatar.svelte'
  import { store } from '../lib/state.svelte'
  import { membersOf } from '../lib/voice'
  import { avatarColor } from '../lib/avatar'
  import { pop } from '../lib/motion'
  import type { Channel, Id } from '../lib/types'
  import Icon from './Icon.svelte'
  import GoLiveDialog from './GoLiveDialog.svelte'
  import StreamView from './StreamView.svelte'

  let { channel, onMenu }: { channel: Channel; onMenu: (userId: Id, e: MouseEvent) => void } = $props()
  const list = $derived(membersOf(store.voice, channel.id))
  const joined = $derived(store.inVoice(channel.id))
  const watching = $derived(store.watching !== null && list.some((m) => m.userId === store.watching) ? store.watching : null)
  let picking = $state(false)
  const strip = $derived(watching !== null && joined)
  const anyoneSpeaking = $derived(list.some((m) => store.speaking.has(m.userId)))
  // Background glow colours: the first two members' avatar colours, else the accent.
  const glow = $derived(
    list.slice(0, 2).map((m) => avatarColor(store.user(m.userId) ?? { id: m.userId })),
  )
  // Columns by head count: 1 · 2×2 · 3×2 · 4×2 fit the minimum window (900×560) without scrolling.
  const cols = $derived(list.length <= 1 ? 1 : list.length <= 4 ? 2 : list.length <= 6 ? 3 : Math.min(5, Math.ceil(list.length / 2)))
  const avatarSize = $derived(strip ? 36 : list.length > 4 ? 56 : 72)

  function share() {
    if (store.live) store.stopLive()
    else picking = true
  }

  function open(user: Id) {
    if (!joined || !store.isLive(user) || user === store.me?.id) return
    store.watch(user)
  }
</script>

<section class="stage" class:anyone-speaking={anyoneSpeaking} style="--g2:{glow[0] ?? 'var(--accent)'};--g3:{glow[1] ?? glow[0] ?? 'var(--accent)'}">
  <div class="glow" aria-hidden="true"></div>
  {#if watching !== null && joined}
    {#key watching}<StreamView userId={watching} />{/key}
  {/if}
  {#if list.length}
    <div class="grid" class:strip style="--cols:{cols}">
      {#each list as m (m.userId)}
        {@const streaming = store.isLive(m.userId)}
        <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
        <div
          transition:pop
          class="tile"
          class:talk={store.speaking.has(m.userId)}
          class:watchable={streaming && joined && m.userId !== store.me?.id}
          title={streaming && joined && m.userId !== store.me?.id ? `Watch ${store.nick(m.userId)}'s stream` : undefined}
          onclick={() => open(m.userId)}
          oncontextmenu={(e) => { e.preventDefault(); onMenu(m.userId, e) }}
        >
          {#if streaming}<span class="badge">LIVE</span>{/if}
          <Avatar user={store.user(m.userId)} id={m.userId} size={avatarSize} speaking={store.speaking.has(m.userId)} />
          <span class="n">
            {store.nick(m.userId)}
            {#if m.deafened}<Icon name="headphonesOff" size={14} />{:else if m.muted}<Icon name="micOff" size={14} />{/if}
          </span>
        </div>
      {/each}
    </div>
  {:else}
    <p class="empty muted">Nobody is here yet.</p>
  {/if}

  {#if joined}
    {#if store.voicePath === 'ws'}<p class="path muted">via server (WebSocket)</p>{/if}
    {#if store.live}
      <p class="path muted">
        You're live{store.live.viewers.length ? ` · ${store.live.viewers.length} watching` : ''}{store.live.codec ? ` · ${store.live.codec.toUpperCase()}` : ''}{store.live.relay ? ' · via server (1080p30 cap)' : ''}
      </p>
    {/if}
    <div class="bar">
      <button class="ctl" class:off={store.myVoice.muted} title={store.myVoice.muted ? 'Unmute' : 'Mute'} onclick={() => store.toggleMute()}>
        <Icon name={store.myVoice.muted ? 'micOff' : 'mic'} />
      </button>
      <button class="ctl" class:off={store.myVoice.deafened} title={store.myVoice.deafened ? 'Undeafen' : 'Deafen'} onclick={() => store.toggleDeafen()}>
        <Icon name={store.myVoice.deafened ? 'headphonesOff' : 'headphones'} />
      </button>
      <button class="ctl" class:on={!!store.live} title={store.live ? 'Stop streaming' : 'Share your screen'} onclick={share}><Icon name="screen" /></button>
      <button class="ctl leave" title="Leave voice" onclick={() => store.leaveVoice()}><Icon name="leave" /></button>
    </div>
  {:else}
    <div class="bar"><button class="btn primary" onclick={() => store.joinVoice(channel.id)}>Join voice</button></div>
  {/if}
</section>

{#if picking}<GoLiveDialog onClose={() => (picking = false)} />{/if}

<style>
  .stage { position: relative; isolation: isolate; overflow: hidden; flex: 1; display: flex; flex-direction: column; min-height: 0; padding: 18px; gap: 14px; }
  /* Slow drifting glow behind the tiles. Moved with transform only (compositor), so it costs no repaints. */
  .glow {
    position: absolute; inset: -30%; z-index: -1; pointer-events: none; opacity: 0.55; transition: opacity 600ms;
    background:
      radial-gradient(closest-side at 30% 35%, color-mix(in srgb, var(--accent) 38%, transparent), transparent),
      radial-gradient(closest-side at 70% 65%, color-mix(in srgb, var(--g2) 32%, transparent), transparent),
      radial-gradient(closest-side at 65% 25%, color-mix(in srgb, var(--g3) 26%, transparent), transparent);
    animation: drift 20s ease-in-out infinite alternate;
  }
  .anyone-speaking .glow { opacity: 0.9; }
  @keyframes drift {
    from { transform: translate(-6%, -4%) rotate(0deg); }
    to { transform: translate(6%, 5%) rotate(12deg); }
  }
  /* Tiles have a size cap and the grid is centred, so the circle's member counts never scroll. */
  .grid { flex: 1; min-height: 0; display: grid; grid-template-columns: repeat(var(--cols), minmax(0, 240px)); gap: 12px; align-content: center; justify-content: center; }
  .tile {
    position: relative; aspect-ratio: 16 / 10; background: rgba(29, 30, 37, 0.72); border-radius: 12px;
    display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 8px;
    box-shadow: inset 0 0 0 1px var(--line); transition: box-shadow 120ms;
  }
  .tile.talk { box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.18); }
  .tile.watchable { cursor: pointer; }
  .tile.watchable:hover { background: var(--elev); }
  .badge { position: absolute; top: 8px; left: 8px; background: var(--danger); color: #fff; font-size: 11px; font-weight: 700; padding: 1px 6px; border-radius: 4px; z-index: 2; }
  .grid.strip { flex: 0 0 auto; grid-template-columns: repeat(auto-fill, minmax(120px, 160px)); justify-content: center; }
  .grid.strip .tile { aspect-ratio: 16 / 9; }
  @media (prefers-reduced-motion: reduce) {
    .glow { animation: none; }
  }
  .n { position: relative; z-index: 2; display: flex; align-items: center; gap: 6px; font-weight: 600; color: var(--text); max-width: 90%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .empty { flex: 1; display: grid; place-items: center; }
  .path { text-align: center; font-size: 12px; margin: 0; }
  .bar { display: flex; justify-content: center; gap: 10px; }
  .ctl { width: 46px; height: 46px; border-radius: 50%; background: var(--elev); display: grid; place-items: center; color: var(--text); }
  .ctl:hover:not(:disabled) { background: var(--elev-2); }
  .ctl.off { background: rgba(239, 68, 68, 0.16); }
  .ctl.on { background: var(--accent); color: #fff; }
  .ctl:disabled { opacity: 0.4; cursor: default; }
  .ctl.leave { background: var(--danger); color: #fff; }
</style>
