<script lang="ts">
  import Icon from './Icon.svelte'
  import Composer from './Composer.svelte'
  import MessageList from './MessageList.svelte'
  import { store } from '../lib/state.svelte'
  import type { Channel, Id } from '../lib/types'
  import VoiceStage from './VoiceStage.svelte'
  import MemberList from './MemberList.svelte'
  import { fade, fly } from 'svelte/transition'
  import Avatar from './Avatar.svelte'
  import CallTimer from './CallTimer.svelte'
  import { membersOf } from '../lib/voice'
  import MiniPlayer from './MiniPlayer.svelte'
  import { musicPanel } from '../lib/panel.svelte'
  import { cubicOut } from 'svelte/easing'
  import { isTablet, layout } from '../lib/layout.svelte'
  import { pane } from '../lib/pane.svelte'
  import { phonePortal } from '../lib/portal'
  import { sheet } from '../lib/motion'

  let { channel, onMemberMenu }: { channel: Channel; onMemberMenu: (userId: Id, e: MouseEvent) => void } = $props()

  let now = $state(Date.now())
  $effect(() => {
    const t = setInterval(() => (now = Date.now()), 1000)
    return () => clearInterval(t)
  })
  const typers = $derived(store.typingNames(channel.id, now))

  // Who's in this chat: a stack of faces in the header; click it for the full list.
  const around = $derived(store.people().filter((u) => store.online.has(u.id) || u.id === store.me?.id))
  let showMembers = $state(false)
  const joinedHere = $derived(channel.kind === 'voice' && store.inVoice(channel.id))
  const music = $derived(channel.kind === 'voice' ? store.musicIn(channel.id) : null)
  // Panel hidden and a song playing: the mini player stands in for the Music button. On a phone the
  // header has no room for it, so it becomes a slim full-width bar under the header instead.
  const mini = $derived(joinedHere && !musicPanel.open && !!music)
  // The call's encryption details exist only for the call I'm in.
  const sec = $derived(joinedHere ? store.security : null)
  const codeGroups = $derived(sec ? sec.code.split(/\s+/).filter(Boolean) : [])
  let showCode = $state(false)
  $effect(() => {
    if (!sec) showCode = false
  })
  /** The call code: a small drop-down by the chip, or a sheet from the bottom on a phone. */
  const codeMotion = (node: Element) => (pane.narrow ? sheet(node, {}, { direction: 'both' }) : fly(node, { y: -4, duration: 160 }))
  const inRoom = $derived(channel.kind === 'voice' ? membersOf(store.voice, channel.id).length : 0)
</script>

<header class:phone={pane.narrow} class="top" class:compact={channel.kind === 'voice'}>
  {#if channel.kind === 'text'}
    {#if pane.narrow}<button class="away" title="Back to chats & rooms" aria-label="Back to chats & rooms" onclick={pane.close}><Icon name="back" size={22} /></button>{/if}
    <span class="ico"><Icon name="hash" size={22} /></span>
    <h1 class="title-xl">{channel.name}</h1>
    {#if channel.topic}<span class="topic">{channel.topic}</span>{/if}
    <button class="crowd" class:active={showMembers} title={showMembers ? 'Hide people' : 'Show people'} aria-pressed={showMembers} onclick={() => (showMembers = !showMembers)}>
      <span class="stack">{#each around.slice(0, 4) as u (u.id)}<span class="sa"><Avatar user={u} id={u.id} size={24} /></span>{/each}</span>
      <span class="count">{around.length} here</span>
    </button>
  {:else}
    {#if isTablet}
      <button class="sidetoggle" aria-pressed={layout.sideHidden} title={layout.sideHidden ? 'Show the sidebar' : 'Hide the sidebar'} onclick={() => (layout.sideHidden = !layout.sideHidden)}><Icon name="sidebar" size={18} /></button>
    {/if}
    {#if pane.narrow}<button class="away" title="Back to chats & rooms" aria-label="Back to chats & rooms" onclick={pane.close}><Icon name="down" size={22} /></button>{/if}
    <span class="ico"><Icon name="volume" size={22} /></span>
    <h1 class="title-xl">{channel.name}</h1>
    {#if inRoom}
      <span class="callchip">
        <button class="chip" class:open={showCode} disabled={!sec} aria-expanded={sec ? showCode : undefined}
          title={sec ? 'End-to-end encrypted. Click for the call code' : 'End-to-end encrypted'}
          onclick={() => (showCode = !showCode)}>
          <Icon name="lock" size={12} /><CallTimer channelId={channel.id} />
        </button>
        {#if showCode && sec}
          <!-- On a phone it's a bottom sheet (moved to <body>, so the floating header can't clip it). -->
          {#if pane.narrow}
            <div class="codescrim" role="presentation" use:phonePortal onpointerdown={() => (showCode = false)} transition:fade={{ duration: 220 }}></div>
          {/if}
          <div class="codepop" role="dialog" aria-label="Call code" use:phonePortal transition:codeMotion>
            {#if pane.narrow}<span class="grab" aria-hidden="true"></span>{/if}
            <b><Icon name="lock" size={14} /> End-to-end encrypted</b>
            <div class="code">{#each codeGroups as g}<span>{g}</span>{/each}</div>
            <p>Everyone in this call sees the same code. If yours matches theirs, nobody is listening in between.</p>
          </div>
        {/if}
      </span>
    {/if}
    <!-- (On a phone with the mini player in its bar there's nothing here: no empty slot taking a gap.) -->
    {#if joinedHere && !(mini && pane.narrow)}
      <div class="tools">
        <!-- One control: while the panel is hidden and a song plays, the mini player stands in for the button. -->
        <!-- Both sit in one cell and cross-fade, so the header never shows a gap while they swap. -->
        {#if mini && music}
          <div class="minislot" in:fly={{ x: 24, duration: 380, delay: 60, easing: cubicOut }} out:fade={{ duration: 140 }}>
            <MiniPlayer {music} onOpen={() => (musicPanel.open = true)} />
          </div>
        {:else}
          <button class="crowd panel-toggle" in:fade={{ duration: 220, delay: 60 }} out:fade={{ duration: 140 }} class:active={musicPanel.open} aria-pressed={musicPanel.open} title={musicPanel.open ? 'Hide the music panel' : 'Show the music panel'} onclick={() => (musicPanel.open = !musicPanel.open)}>
            <Icon name="music" size={16} /><span class="count">Music</span>
          </button>
        {/if}
      </div>
    {/if}
  {/if}
  {#if mini && pane.narrow && music}
    <div class="minibar" transition:fly={{ y: -10, duration: 320, easing: cubicOut }}>
      <MiniPlayer {music} onOpen={() => (musicPanel.open = true)} />
    </div>
  {/if}
</header>

{#if channel.kind === 'voice'}
  <div class="stage"><VoiceStage {channel} onMenu={onMemberMenu} /></div>
{:else}
  <div class="body">
    <div class="chat">
      <MessageList channel={channel.id} />
      <div class="typing muted">
        {#if typers.length === 1}{typers[0]} is typing…{:else if typers.length > 1}{typers.join(', ')} are typing…{/if}
      </div>
      <Composer {channel} />
    </div>
    {#if showMembers}<div class="side" transition:fly={{ x: 24, duration: 200 }}><MemberList onMenu={onMemberMenu} /></div>{/if}
  </div>
{/if}

<style>
  .top { display: flex; align-items: center; gap: 10px; padding: 16px 22px 12px; min-height: 64px; }
  .ico { color: var(--accent); display: inline-flex; }
  /* A room's header stays slim: the room itself is the show. */
  /* It floats over the room with no band of its own: a soft shade at the very top that fades out, so
     the room (and the song's backdrop) runs right up under it. */
  .top.compact {
    position: absolute; top: 0; left: 0; right: 0; z-index: 5; padding: 10px 18px 22px; min-height: 52px;
    background: linear-gradient(to bottom, color-mix(in srgb, var(--bg) 82%, transparent), color-mix(in srgb, var(--bg) 40%, transparent) 55%, transparent);
  }
  .top.compact h1 { font-size: 22px; }
  h1 { margin: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: min(5.5em, 30vw); }
  .callchip { position: relative; flex: none; }
  /* Phones: the way back to the list, at the start of the title. */
  .away { flex: none; width: 36px; height: 36px; margin: 0 -6px 0 -10px; border-radius: 10px; display: grid; place-items: center; color: var(--text); }
  .sidetoggle { flex: none; width: 36px; height: 36px; margin-left: -6px; border-radius: 10px; display: grid; place-items: center; color: var(--muted); background: var(--elev-2); border: 1px solid var(--line); }
  .sidetoggle[aria-pressed='true'] { color: var(--accent); }
  .chip { display: inline-flex; align-items: center; gap: 6px; height: 26px; padding: 0 10px; border-radius: 999px; background: var(--elev-2); border: 1px solid var(--line); color: var(--muted); font-size: 12.5px; font-weight: 600; }
  .chip:not(:disabled):hover, .chip.open { color: var(--text); border-color: #43384f; }
  .chip:disabled { cursor: default; opacity: 1; }
  .codepop { position: absolute; top: calc(100% + 8px); left: 0; z-index: 40; width: 300px; padding: 12px 14px; border-radius: 12px; background: var(--panel); border: 1px solid var(--line); box-shadow: 0 18px 40px rgba(0, 0, 0, 0.35); }
  .codepop b { font-size: 13px; }
  .codepop .code { margin-top: 8px; display: flex; flex-wrap: wrap; gap: 2px 10px; font-family: ui-monospace, 'SF Mono', Menlo, monospace; font-size: 13px; font-variant-numeric: tabular-nums; user-select: text; }
  .codepop p { margin: 8px 0 0; font-size: 12px; color: var(--muted); }
  .codepop b { display: inline-flex; align-items: center; gap: 6px; }

  /* Phones: the header row spreads out, the call chip at its far end; the mini player hangs under it. */
  .top.phone .callchip { margin-left: auto; }
  .top.phone .tools { margin-left: 0; }
  .top.phone .minibar { left: 14px; right: 14px; top: calc(100% - 12px); }
  @media (max-width: 760px) {
    .codescrim { position: fixed; inset: 0; z-index: 300; background: rgba(0, 0, 0, 0.45); }
    .codepop {
      position: fixed; top: auto; left: 0; right: 0; bottom: 0; z-index: 301; width: auto;
      padding: 10px 20px calc(22px + env(safe-area-inset-bottom)); border-radius: 18px 18px 0 0; border: 0;
      background: rgba(20, 20, 28, 0.98); color: #fff; box-shadow: 0 -12px 40px rgba(0, 0, 0, 0.45);
    }
    .codepop .grab { display: block; width: 38px; height: 4px; margin: 0 auto 14px; border-radius: 2px; background: rgba(255, 255, 255, 0.22); }
    .codepop b { font-size: 15px; color: var(--accent); }
    .codepop .code { margin-top: 14px; gap: 8px 16px; font-size: 20px; font-weight: 600; letter-spacing: 0.04em; justify-content: center; }
    .codepop p { margin-top: 14px; font-size: 13.5px; line-height: 1.45; }
  }
  .topic { color: var(--muted); font-size: 13.5px; padding-left: 4px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .crowd { margin-left: auto; flex: none; display: flex; align-items: center; gap: 8px; padding: 4px 10px 4px 5px; border-radius: 999px; background: var(--elev-2); border: 1px solid var(--line); color: var(--muted); font-size: 12.5px; font-weight: 600; }
  .crowd:hover, .crowd.active { color: var(--text); border-color: #43384f; }
  /* The tools give way before the channel name: a long song title shortens with an ellipsis. */
  .tools { margin-left: auto; flex: 0 1 auto; display: grid; grid-template-columns: minmax(0, max-content); align-items: center; justify-items: end; min-width: 0; }
  .tools > * { grid-area: 1 / 1; }
  .tools .crowd { margin-left: 0; height: 32px; padding: 0 12px 0 10px; transition: background 200ms, color 200ms, border-color 200ms; }
  .panel-toggle.active { color: var(--accent); background: var(--accent-soft); border-color: transparent; }
  .minislot { min-width: 0; max-width: 100%; }
  .minislot > :global(.mini) { max-width: 100%; }
  .stack { display: flex; }
  .sa { display: inline-flex; border-radius: 50%; box-shadow: 0 0 0 2px var(--elev-2); }
  .sa + .sa { margin-left: -8px; }
  .body { position: relative; flex: 1; display: flex; min-height: 0; }
  .chat { flex: 1; min-width: 0; display: flex; flex-direction: column; }
  .side { position: absolute; top: 0; right: 14px; bottom: 96px; z-index: 6; display: flex; border-radius: 14px; overflow: hidden; box-shadow: 0 0 0 1px var(--line), 0 18px 40px rgba(0, 0, 0, 0.45); }
  /* Hangs just under the floating header, across the screen. */
  .minibar { position: absolute; left: 12px; right: 12px; top: calc(100% - 16px); z-index: 1; }
  .minibar > :global(.mini) { max-width: none; height: 46px; border-radius: 14px; }
  .stage { flex: 1; min-height: 0; display: flex; flex-direction: column; }
  /* Room for the floating header above. */
  .stage > :global(.stage) { padding-top: 60px; }
  .typing { height: 20px; padding: 0 24px; font-size: 12px; }
  /* Phones: the Back bar sits above, so the header stays short. */
  @media (max-width: 760px) {
    .top:not(.compact) { padding: 10px 16px 8px; min-height: 0; }
    .top:not(.compact) h1 { font-size: 24px; }
    .topic { display: none; }
  }
</style>
