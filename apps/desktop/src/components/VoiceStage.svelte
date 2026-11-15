<script lang="ts">
  import Avatar from './Avatar.svelte'
  import { store } from '../lib/state.svelte'
  import { membersOf } from '../lib/voice'
  import { dockIn, pop, sidePanel } from '../lib/motion'
  import { musicPanel } from '../lib/panel.svelte'
  import { seats } from '../lib/bento'
  import { groupTitle } from '../lib/home'
  import type { Channel, Id } from '../lib/types'
  import Icon from './Icon.svelte'
  import SwapLabel from './SwapLabel.svelte'
  import GoLiveDialog from './GoLiveDialog.svelte'
  import StreamView from './StreamView.svelte'
  import MusicPlayer from './MusicPlayer.svelte'
  import MusicBackdrop from './MusicBackdrop.svelte'
  import MusicIdle from './MusicIdle.svelte'

  // A room, not a video grid: everyone sits round the lamp as a face with their name under it.
  // Whoever is talking gets lit; someone sharing their screen brings a screen to the table.
  let { channel, onMenu }: { channel: Channel; onMenu: (userId: Id, e: MouseEvent) => void } = $props()
  const list = $derived(membersOf(store.voice, channel.id))
  const joined = $derived(store.inVoice(channel.id))
  const watching = $derived(store.watching !== null && list.some((m) => m.userId === store.watching) ? store.watching : null)
  let picking = $state(false)
  const music = $derived(store.musicIn(channel.id))
  /** The music panel sits on the right (empty when nothing plays), unless a stream is open (the stream wins). */
  const playerUp = $derived(joined && watching === null)
  const strip = $derived(watching !== null && joined)
  const anyoneSpeaking = $derived(list.some((m) => store.speaking.has(m.userId)))
  const meId = $derived(store.me?.id)

  const GAP = 28
  let gw = $state(0)
  let gh = $state(0)
  // A shared screen is about two seats wide, so it counts as two when sizing the room.
  const screens = $derived(list.filter((m) => store.isLive(m.userId)).length)
  const seat = $derived(strip ? { cols: list.length, size: 44 } : seats(list.length + screens, gw - 24, gh - 24, GAP, playerUp ? 132 : 168))

  const who = $derived(groupTitle(list.map((m) => store.nick(m.userId))))

  function share() {
    if (store.live) store.stopLive()
    else picking = true
  }

  function open(user: Id) {
    if (!joined || !store.isLive(user) || user === meId) return
    store.watch(user)
  }
</script>

<section class="stage" class:lit={anyoneSpeaking}>
  {#if music}<MusicBackdrop url={music.state.now!.artUrl} />{:else}<div class="lamp" aria-hidden="true"></div>{/if}
  {#if !joined}
    <!-- Before going in: who's inside, and the way in. -->
    <div class="door">
      <div class="inside" class:empty={!list.length}>
        {#each list.slice(0, 5) as m (m.userId)}
          <span class="pk" transition:pop><Avatar user={store.user(m.userId)} id={m.userId} size={64} speaking={store.speaking.has(m.userId)} /></span>
        {/each}
        {#if list.length > 5}<span class="pk more">+{list.length - 5}</span>{/if}
        {#if !list.length}<span class="chair"><Icon name="volume" size={26} /></span>{/if}
      </div>
      <p class="who">{list.length ? `${who} ${list.length === 1 ? 'is' : 'are'} in here` : 'Nobody is in here yet'}</p>
      {#if music}<p class="np"><Icon name="music" size={14} /> {music.state.now!.title} by {music.state.now!.artist}</p>{/if}
      <button class="enter" onclick={() => store.joinVoice(channel.id)}>{list.length ? 'Join them' : 'Go in'}</button>
    </div>
  {:else}
    {#snippet people()}
      <div class="room" class:strip bind:clientWidth={gw} bind:clientHeight={gh}>
        <div class="seats" style="--sz:{seat.size}px;--gap:{strip ? 14 : GAP}px;{strip ? '' : `max-width:${seat.cols * (seat.size + GAP)}px`}">
          {#each list as m (m.userId)}
            {@const streaming = store.isLive(m.userId)}
            {@const talking = store.speaking.has(m.userId)}
            {@const mine = m.userId === meId}
            <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
            <div
              transition:pop
              class="seat"
              class:talk={talking}
              class:me={mine}
              class:screen={streaming && !strip}
              class:watchable={streaming && !mine}
              title={streaming && !mine ? `Watch ${store.nick(m.userId)}'s screen` : undefined}
              onclick={() => open(m.userId)}
              oncontextmenu={(e) => { e.preventDefault(); onMenu(m.userId, e) }}
            >
              {#if streaming && !strip}
                <!-- Their screen, on the table. -->
                <div class="tv">
                  <span class="live">Live</span>
                  <span class="tva">
                    <Avatar user={store.user(m.userId)} id={m.userId} size={36} speaking={talking} />
                    {#if m.deafened}<span class="state sm" title="Can't hear"><Icon name="headphonesOff" size={10} /></span>
                    {:else if m.muted}<span class="state sm" title="Muted"><Icon name="micOff" size={10} /></span>{/if}
                  </span>
                  {#if mine}
                    <span class="tvl">You're sharing your screen{store.live?.viewers.length ? `, ${store.live.viewers.length} watching` : ''}</span>
                  {:else}
                    <button class="watch" onclick={(e) => { e.stopPropagation(); open(m.userId) }}><Icon name="screen" size={15} /> Watch</button>
                  {/if}
                </div>
              {:else}
                <span class="face">
                  <Avatar user={store.user(m.userId)} id={m.userId} size={seat.size} speaking={talking} />
                  {#if m.deafened}<span class="state" title="Can't hear"><Icon name="headphonesOff" size={13} /></span>
                  {:else if m.muted}<span class="state" title="Muted"><Icon name="micOff" size={13} /></span>{/if}
                  {#if streaming}<span class="state live-dot" title="Sharing their screen"></span>{/if}
                </span>
              {/if}
              {#if !strip}<span class="n">{mine ? 'You' : store.nick(m.userId)}</span>{/if}
            </div>
          {/each}
        </div>
      </div>
    {/snippet}
    {#if watching !== null}
      {#key watching}<StreamView userId={watching} />{/key}
      {#if music}<p class="np"><Icon name="music" size={14} /> {music.state.now!.title} by {music.state.now!.artist}</p>{/if}
      {@render people()}
    {:else if playerUp}
      <div class="split">
        {@render people()}
        {#if musicPanel.open}
          <div class="side" transition:sidePanel>
            {#if music}<MusicPlayer {music} />{:else}<MusicIdle channelName={channel.name} />{/if}
          </div>
        {/if}
      </div>
    {:else}
      {@render people()}
    {/if}

    <div class="dock-row">
      {#if store.voicePath === 'ws' || store.live}
        <p class="path">
          {#if store.live}You're live{store.live.viewers.length ? `, ${store.live.viewers.length} watching` : ''}{store.live.codec ? ` (${store.live.codec.toUpperCase()}${store.live.relay ? ', via server, 1080p30 cap' : ''})` : store.live.relay ? ' (via server, 1080p30 cap)' : ''}{/if}
          {#if store.voicePath === 'ws'}{store.live ? '. ' : ''}Voice is going through the server{/if}
        </p>
      {/if}
      <div class="dock" role="toolbar" aria-label="Call" in:dockIn>
        <button class="ctl" style="--i:0" class:off={store.myVoice.muted} aria-pressed={store.myVoice.muted} aria-label={store.myVoice.muted ? 'Unmute' : 'Mute'} onclick={() => store.toggleMute()}>
          <SwapLabel icons={['mic', 'micOff']} labels={['Mic', 'Muted']} flipped={store.myVoice.muted} />
        </button>
        <button class="ctl" style="--i:1" class:off={store.myVoice.deafened} aria-pressed={store.myVoice.deafened} aria-label={store.myVoice.deafened ? 'Undeafen' : 'Deafen'} onclick={() => store.toggleDeafen()}>
          <SwapLabel icons={['headphones', 'headphonesOff']} labels={['Sound', 'Deafened']} flipped={store.myVoice.deafened} />
        </button>
        <button class="ctl" style="--i:2" class:on={!!store.live} aria-pressed={!!store.live} aria-label={store.live ? 'Stop sharing' : 'Share screen'} onclick={share}>
          <SwapLabel icons={['screen', 'screen']} labels={['Share screen', 'Stop sharing']} flipped={!!store.live} />
        </button>
        <span class="sep" style="--i:3" aria-hidden="true"></span>
        <button class="ctl leave" style="--i:4" onclick={() => store.leaveVoice()}><Icon name="leave" size={18} /><span class="lbl">Leave</span></button>
      </div>
    </div>
  {/if}
</section>

{#if picking}<GoLiveDialog onClose={() => (picking = false)} />{/if}

<style>
  .stage { position: relative; isolation: isolate; overflow: hidden; flex: 1; display: flex; flex-direction: column; min-height: 0; padding: 4px 18px 16px; gap: 12px; }
  /* The lamp: a warm pool of light on the floor of the room, brighter while someone talks. */
  .lamp {
    position: absolute; inset: 0; z-index: -1; pointer-events: none; opacity: 0.7; transition: opacity 700ms ease-out;
    background:
      radial-gradient(60% 55% at 50% 58%, rgba(244, 184, 96, 0.13), transparent 70%),
      radial-gradient(90% 40% at 50% 105%, rgba(244, 184, 96, 0.1), transparent 70%);
  }
  .lit .lamp { opacity: 1; }

  /* Outside the room. */
  .door { margin: auto; display: flex; flex-direction: column; align-items: center; text-align: center; gap: 4px; padding: 20px; }
  .inside { display: flex; margin-bottom: 18px; }
  .pk { margin-left: -14px; border-radius: 50%; box-shadow: 0 0 0 4px var(--bg); display: inline-flex; }
  .pk:first-child { margin-left: 0; }
  .pk.more { width: 64px; height: 64px; display: grid; place-items: center; background: var(--elev); color: var(--muted); font-weight: 700; }
  .chair { width: 72px; height: 72px; border-radius: 50%; display: grid; place-items: center; color: var(--faint); border: 2px dashed var(--line); }
  .who { margin: 0 0 16px; color: var(--muted); font-size: 15px; }
  .np { margin: 0; display: flex; align-items: center; justify-content: center; gap: 6px; color: var(--muted); font-size: 13px; }
  .door .np { margin: -10px 0 16px; }
  .enter {
    background: var(--accent); color: var(--on-accent); font-weight: 700; font-size: 15px; padding: 11px 26px; border-radius: 999px;
    box-shadow: 0 8px 28px rgba(244, 184, 96, 0.22); transition: transform 140ms, box-shadow 140ms;
  }
  .enter:hover { transform: translateY(-1px); box-shadow: 0 10px 32px rgba(244, 184, 96, 0.32); }

  /* Inside: seats round the lamp. */
  .split { flex: 1; min-height: 0; display: flex; gap: 16px; }
  .room { flex: 1; min-height: 0; min-width: 0; display: grid; place-items: center; }
  .room.strip { flex: 0 0 auto; display: block; }
  .seats { display: flex; flex-wrap: wrap; justify-content: center; align-items: flex-start; gap: 18px var(--gap); }
  .room.strip .seats { justify-content: flex-start; }
  .seat { display: flex; flex-direction: column; align-items: center; gap: 10px; width: var(--sz); }
  .face { position: relative; display: inline-flex; border-radius: 50%; transition: box-shadow 220ms ease-out, transform 220ms ease-out; }
  /* Talking: lit by the lamp. */
  .seat.talk .face { box-shadow: 0 0 0 3px var(--bg), 0 0 0 6px var(--accent), 0 0 40px rgba(244, 184, 96, 0.35); transform: scale(1.03); }
  .state {
    position: absolute; right: 4%; bottom: 4%; z-index: 2; width: 28px; height: 28px; border-radius: 50%; display: grid; place-items: center;
    background: var(--elev); color: var(--danger); box-shadow: 0 0 0 3px var(--bg);
  }
  .state.live-dot { right: auto; left: 4%; width: 16px; height: 16px; background: var(--danger); }
  .room.strip .state { width: 18px; height: 18px; right: -4px; bottom: -4px; }
  .room.strip .state :global(svg) { width: 10px; height: 10px; }
  .room.strip .state.live-dot { width: 10px; height: 10px; left: -1px; top: -1px; right: auto; bottom: auto; box-shadow: 0 0 0 2px var(--bg); }
  .n { max-width: 100%; font-weight: 650; font-size: 15px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .seat.me .n { color: var(--accent); }
  .seat.watchable { cursor: pointer; }

  /* Someone's screen: a little TV on the table, as wide as two seats. */
  .seat.screen { width: calc(var(--sz) * 1.78); }
  .tv {
    position: relative; width: 100%; aspect-ratio: 16 / 10; border-radius: 18px; display: grid; place-items: center; align-content: center; gap: 12px;
    background: linear-gradient(160deg, var(--elev), var(--panel-2)); box-shadow: inset 0 0 0 1px var(--line), 0 18px 40px rgba(0, 0, 0, 0.35);
    transition: transform 160ms ease-out;
  }
  .seat.talk .tv { box-shadow: inset 0 0 0 2px var(--accent), 0 18px 40px rgba(0, 0, 0, 0.35); }
  .seat.watchable:hover .tv { transform: translateY(-2px); }
  .live { position: absolute; top: 10px; left: 12px; background: var(--danger); color: #fff; font-size: 11.5px; font-weight: 700; padding: 1px 8px; border-radius: 999px; }
  .tva { position: relative; display: inline-flex; }
  .state.sm { width: 18px; height: 18px; right: -5px; bottom: -5px; box-shadow: 0 0 0 2px var(--elev); }
  .tvl { padding: 0 14px; color: var(--muted); font-size: 13px; text-align: center; }
  .watch { display: inline-flex; align-items: center; gap: 7px; padding: 7px 16px; border-radius: 999px; background: var(--accent); color: var(--on-accent); font-weight: 700; font-size: 14px; }

  /* The call controls: one pill under the room, each button saying what it does. */
  .dock-row { display: flex; flex-direction: column; align-items: center; gap: 8px; }
  .path { margin: 0; font-size: 12px; color: var(--faint); text-align: center; }
  .dock { display: flex; align-items: center; gap: 4px; padding: 6px; border-radius: 999px; background: var(--panel-2); box-shadow: inset 0 0 0 1px var(--line), 0 12px 30px rgba(0, 0, 0, 0.3); }
  .ctl {
    display: inline-flex; align-items: center; gap: 8px; height: 40px; padding: 0 16px; border-radius: 999px; color: var(--text); font-weight: 600; font-size: 13.5px;
    transition: background 280ms ease, color 280ms ease, transform 160ms ease;
    animation: ctl-in 520ms cubic-bezier(0.22, 1, 0.36, 1) both; animation-delay: calc(160ms + var(--i) * 55ms);
  }
  .ctl:active { transform: scale(0.95); }
  .sep { animation: ctl-in 520ms both; animation-delay: calc(160ms + var(--i) * 55ms); }
  @keyframes ctl-in { from { opacity: 0; transform: translateY(8px); } }
  .ctl:hover { background: var(--elev); }
  .ctl.off { color: var(--danger); background: rgba(242, 84, 91, 0.12); }
  .ctl.on { background: var(--accent); color: var(--on-accent); }
  .ctl.leave { color: #fff; background: var(--danger); }
  .ctl.leave:hover { background: #e2444b; }
  .sep { width: 1px; height: 22px; background: var(--line); margin: 0 4px; }
  @media (max-width: 720px) { .ctl .lbl { display: none; } .ctl { padding: 0 12px; } }
  /* The panel rides on its slot's left edge, so as the slot opens or closes the whole panel slides in
     from (or out past) the right edge rather than being squeezed. */
  .side { flex: none; width: clamp(320px, 34%, 460px); min-height: 0; display: flex; justify-content: flex-start; }
  @media (prefers-reduced-motion: reduce) {
    .lamp, .face, .tv, .ctl { transition: none; }
    .ctl, .sep { animation: none; }
  }
</style>
