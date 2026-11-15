<script lang="ts">
  import Avatar from './Avatar.svelte'
  import { store } from '../lib/state.svelte'
  import { membersOf } from '../lib/voice'
  import { avatarColor } from '../lib/avatar'
  import { pop } from '../lib/motion'
  import { bento } from '../lib/bento'
  import { groupTitle } from '../lib/home'
  import type { Channel, Id } from '../lib/types'
  import Icon from './Icon.svelte'
  import GoLiveDialog from './GoLiveDialog.svelte'
  import StreamView from './StreamView.svelte'
  import MusicPlayer from './MusicPlayer.svelte'
  import MusicBackdrop from './MusicBackdrop.svelte'

  let { channel, onMenu }: { channel: Channel; onMenu: (userId: Id, e: MouseEvent) => void } = $props()
  const list = $derived(membersOf(store.voice, channel.id))
  const joined = $derived(store.inVoice(channel.id))
  const watching = $derived(store.watching !== null && list.some((m) => m.userId === store.watching) ? store.watching : null)
  let picking = $state(false)
  const music = $derived(store.musicIn(channel.id))
  /** The music panel sits on the right, unless a stream is open (the stream wins). */
  const playerUp = $derived(joined && music !== null && watching === null)
  const strip = $derived(watching !== null && joined)
  const anyoneSpeaking = $derived(list.some((m) => store.speaking.has(m.userId)))
  // Background glow colours: the first two members' avatar colours, else the accent.
  const glow = $derived(
    list.slice(0, 2).map((m) => avatarColor(store.user(m.userId) ?? { id: m.userId })),
  )
  const colorOf = (id: Id) => avatarColor(store.user(id) ?? { id })

  // Bento grid: tiles as big as the stage allows (16:9), re-laid out when the window resizes.
  const GAP = 10
  let gw = $state(0)
  let gh = $state(0)
  const layout = $derived(bento(list.length, gw, gh, GAP))
  const avatarSize = $derived(strip ? 36 : Math.round(Math.min(112, Math.max(40, layout.h * 0.34))))

  const who = $derived(groupTitle(list.map((m) => store.nick(m.userId))))

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
  {#if music}<MusicBackdrop url={music.state.now!.artUrl} />{:else}<div class="glow" aria-hidden="true"></div>{/if}
  {#if !joined}
    <!-- Before joining: who's in here, and one big button. -->
    <div class="prejoin">
      {#if list.length}
        <div class="peek">
          {#each list.slice(0, 4) as m (m.userId)}
            <span class="pt" style="--c:{colorOf(m.userId)}" transition:pop><Avatar user={store.user(m.userId)} id={m.userId} size={40} speaking={store.speaking.has(m.userId)} /></span>
          {/each}
          {#if list.length > 4}<span class="pt more">+{list.length - 4}</span>{/if}
        </div>
      {/if}
      <h2>{channel.name}</h2>
      <p>{list.length ? `${who} ${list.length === 1 ? 'is' : 'are'} currently in voice` : 'No one is here yet. Be the first!'}</p>
      {#if music}<p class="np"><Icon name="music" size={14} /> {music.state.now!.title} — {music.state.now!.artist}</p>{/if}
      <button class="joinbtn" onclick={() => store.joinVoice(channel.id)}>Join Voice</button>
    </div>
  {:else}
    {#snippet people()}
    <div class="area" class:strip bind:clientWidth={gw} bind:clientHeight={gh}>
      <div class="grid" class:strip style={strip ? '' : `width:${layout.cols * layout.w + (layout.cols - 1) * GAP}px`}>
        {#each list as m (m.userId)}
          {@const streaming = store.isLive(m.userId)}
          {@const talking = store.speaking.has(m.userId)}
          <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
          <div
            transition:pop
            class="tile"
            class:talk={talking}
            class:watchable={streaming && m.userId !== store.me?.id}
            style="--c:{colorOf(m.userId)};{strip ? '' : `width:${layout.w}px;height:${layout.h}px`}"
            title={streaming && m.userId !== store.me?.id ? `Watch ${store.nick(m.userId)}'s stream` : undefined}
            onclick={() => open(m.userId)}
            oncontextmenu={(e) => { e.preventDefault(); onMenu(m.userId, e) }}
          >
            {#if streaming}<span class="badge">LIVE</span>{/if}
            {#if streaming && !strip}
              <div class="live">
                <Avatar user={store.user(m.userId)} id={m.userId} size={Math.min(avatarSize, 56)} speaking={talking} />
                {#if m.userId === store.me?.id}
                  <span class="self"><Icon name="screen" size={16} /> You're sharing your screen</span>
                  {#if store.live?.viewers.length}<span class="sub">{store.live.viewers.length} watching</span>{/if}
                {:else}
                  <button class="watch" onclick={(e) => { e.stopPropagation(); open(m.userId) }}>
                    <Icon name="screen" size={16} /> Watch Stream
                  </button>
                {/if}
              </div>
            {:else}
              <Avatar user={store.user(m.userId)} id={m.userId} size={avatarSize} speaking={talking} />
            {/if}
            <span class="n">
              {#if m.deafened}<Icon name="headphonesOff" size={13} />{:else if m.muted}<Icon name="micOff" size={13} />{/if}
              {store.nick(m.userId)}
            </span>
          </div>
        {/each}
      </div>
    </div>

    {/snippet}
    {#if watching !== null}
      {#key watching}<StreamView userId={watching} />{/key}
      {#if music}<p class="np"><Icon name="music" size={14} /> {music.state.now!.title} — {music.state.now!.artist}</p>{/if}
      {@render people()}
    {:else if playerUp && music}
      <div class="split">
        {@render people()}
        <MusicPlayer {music} />
      </div>
    {:else}
      {@render people()}
    {/if}

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
  {/if}
</section>

{#if picking}<GoLiveDialog onClose={() => (picking = false)} />{/if}

<style>
  .stage {
    position: relative; isolation: isolate; overflow: hidden; flex: 1; display: flex; flex-direction: column; min-height: 0; padding: 16px; gap: 12px;
    background: radial-gradient(120% 80% at 50% 115%, color-mix(in srgb, var(--accent) 55%, #2b2fb0) 0%, #20205a 45%, #101018 100%);
  }
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
  .prejoin { margin: auto; display: flex; flex-direction: column; align-items: center; text-align: center; gap: 6px; padding: 20px; }
  .peek { display: flex; gap: 8px; margin-bottom: 18px; }
  .pt { width: 132px; height: 74px; border-radius: 8px; display: grid; place-items: center; background: color-mix(in srgb, var(--c) 45%, #d9d6e8); }
  .pt.more { background: rgba(255, 255, 255, 0.12); color: #fff; font-weight: 700; width: 74px; }
  .prejoin h2 { margin: 0; font-size: 28px; font-weight: 800; color: #fff; }
  .prejoin p { margin: 0 0 14px; color: rgba(255, 255, 255, 0.78); font-size: 14px; }
  .joinbtn { background: #fff; color: #16171c; font-weight: 600; padding: 9px 18px; border-radius: 8px; transition: transform 120ms, box-shadow 120ms; }
  .joinbtn:hover { transform: translateY(-1px); box-shadow: 0 6px 18px rgba(0, 0, 0, 0.35); }

  .area { flex: 1; min-height: 0; min-width: 0; display: grid; place-items: center; }
  .split { flex: 1; min-height: 0; display: flex; gap: 16px; }
  .grid { display: flex; flex-wrap: wrap; justify-content: center; gap: 10px; }
  .tile {
    position: relative; border-radius: 12px; overflow: hidden; display: grid; place-items: center;
    background: color-mix(in srgb, var(--c) 30%, #1a1b22);
    box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.06); transition: box-shadow 140ms;
  }
  .tile.talk { box-shadow: inset 0 0 0 2px color-mix(in srgb, var(--c) 70%, #fff); }
  .tile.watchable { cursor: pointer; }
  .tile.watchable:hover { filter: brightness(1.1); }
  .badge { position: absolute; top: 8px; left: 8px; background: var(--danger); color: #fff; font-size: 11px; font-weight: 700; padding: 1px 6px; border-radius: 4px; z-index: 2; }
  .live { display: flex; flex-direction: column; align-items: center; gap: 10px; text-align: center; padding: 8px; }
  .watch {
    display: inline-flex; align-items: center; gap: 7px; padding: 8px 16px; border-radius: 8px;
    background: rgba(255, 255, 255, 0.92); color: #16171c; font-weight: 600; font-size: 14px;
    transition: transform 120ms, box-shadow 120ms;
  }
  .watch:hover { transform: translateY(-1px); box-shadow: 0 6px 18px rgba(0, 0, 0, 0.35); }
  .self { display: inline-flex; align-items: center; gap: 7px; padding: 6px 12px; border-radius: 8px; background: rgba(0, 0, 0, 0.45); color: #fff; font-weight: 600; font-size: 13px; }
  .sub { color: rgba(255, 255, 255, 0.78); font-size: 12px; }
  .area.strip { flex: 0 0 auto; display: block; }
  .np { margin: 0; display: flex; align-items: center; justify-content: center; gap: 6px; color: rgba(255, 255, 255, 0.85); font-size: 13px; }
  .prejoin .np { margin: -8px 0 14px; }
  .grid.strip { flex-wrap: wrap; }
  .grid.strip .tile { width: 150px; height: 84px; }
  .n {
    position: absolute; left: 8px; bottom: 8px; z-index: 2; max-width: calc(100% - 16px); display: flex; align-items: center; gap: 5px;
    padding: 3px 8px; border-radius: 6px; background: rgba(0, 0, 0, 0.45); color: #fff; font-size: 13px; font-weight: 600;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  @media (prefers-reduced-motion: reduce) {
    .glow { animation: none; }
  }
  .path { text-align: center; font-size: 12px; margin: 0; color: rgba(255, 255, 255, 0.7); }
  .bar { display: flex; justify-content: center; gap: 10px; }
  .ctl { width: 46px; height: 46px; border-radius: 50%; background: rgba(20, 21, 28, 0.8); display: grid; place-items: center; color: var(--text); }
  .ctl:hover:not(:disabled) { background: var(--elev-2); }
  .ctl.off { background: rgba(239, 68, 68, 0.16); }
  .ctl.on { background: var(--accent); color: #fff; }
  .ctl:disabled { opacity: 0.4; cursor: default; }
  .ctl.leave { background: var(--danger); color: #fff; }
</style>
