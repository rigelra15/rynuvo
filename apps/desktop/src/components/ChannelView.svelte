<script lang="ts">
  import Icon from './Icon.svelte'
  import Composer from './Composer.svelte'
  import MessageList from './MessageList.svelte'
  import { store } from '../lib/state.svelte'
  import type { Channel, Id } from '../lib/types'
  import VoiceStage from './VoiceStage.svelte'
  import MemberList from './MemberList.svelte'
  import { fly } from 'svelte/transition'
  import Avatar from './Avatar.svelte'
  import CallTimer from './CallTimer.svelte'
  import { membersOf } from '../lib/voice'
  import MiniPlayer from './MiniPlayer.svelte'
  import { musicPanel } from '../lib/panel.svelte'
  import { cubicOut } from 'svelte/easing'

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
  const inRoom = $derived(channel.kind === 'voice' ? membersOf(store.voice, channel.id).length : 0)
</script>

<header class="top" class:compact={channel.kind === 'voice'}>
  {#if channel.kind === 'text'}
    <span class="ico"><Icon name="hash" size={22} /></span>
    <h1 class="title-xl">{channel.name}</h1>
    {#if channel.topic}<span class="topic">{channel.topic}</span>{/if}
    <button class="crowd" class:active={showMembers} title={showMembers ? 'Hide people' : 'Show people'} aria-pressed={showMembers} onclick={() => (showMembers = !showMembers)}>
      <span class="stack">{#each around.slice(0, 4) as u (u.id)}<span class="sa"><Avatar user={u} id={u.id} size={24} /></span>{/each}</span>
      <span class="count">{around.length} here</span>
    </button>
  {:else}
    <span class="ico"><Icon name="volume" size={22} /></span>
    <h1 class="title-xl">{channel.name}</h1>
    {#if inRoom}<span class="topic">{inRoom} inside<CallTimer channelId={channel.id} prefix=", going for " /></span>{/if}
    {#if joinedHere}
      <div class="tools">
        <!-- One control: while the panel is hidden and a song plays, the mini player stands in for the button. -->
        {#if !musicPanel.open && music}
          <div class="minislot" in:fly={{ x: 24, duration: 380, delay: 200, easing: cubicOut }}>
            <MiniPlayer {music} onOpen={() => (musicPanel.open = true)} />
          </div>
        {:else}
          <button class="crowd panel-toggle" class:active={musicPanel.open} aria-pressed={musicPanel.open} title={musicPanel.open ? 'Hide the music panel' : 'Show the music panel'} onclick={() => (musicPanel.open = !musicPanel.open)}>
            <Icon name="music" size={16} /><span class="count">Music</span>
          </button>
        {/if}
      </div>
    {/if}
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
  h1 { margin: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .topic { color: var(--muted); font-size: 13.5px; padding-left: 4px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .crowd { margin-left: auto; flex: none; display: flex; align-items: center; gap: 8px; padding: 4px 10px 4px 5px; border-radius: 999px; background: var(--elev-2); border: 1px solid var(--line); color: var(--muted); font-size: 12.5px; font-weight: 600; }
  .crowd:hover, .crowd.active { color: var(--text); border-color: #43384f; }
  .tools { margin-left: auto; flex: none; display: flex; align-items: center; gap: 10px; min-width: 0; }
  .tools .crowd { margin-left: 0; height: 32px; padding: 0 12px 0 10px; transition: background 200ms, color 200ms, border-color 200ms; }
  .panel-toggle.active { color: var(--accent); background: var(--accent-soft); border-color: transparent; }
  .minislot { min-width: 0; }
  .stack { display: flex; }
  .sa { display: inline-flex; border-radius: 50%; box-shadow: 0 0 0 2px var(--elev-2); }
  .sa + .sa { margin-left: -8px; }
  .body { position: relative; flex: 1; display: flex; min-height: 0; }
  .chat { flex: 1; min-width: 0; display: flex; flex-direction: column; }
  .side { position: absolute; top: 0; right: 14px; bottom: 96px; z-index: 6; display: flex; border-radius: 14px; overflow: hidden; box-shadow: 0 0 0 1px var(--line), 0 18px 40px rgba(0, 0, 0, 0.45); }
  .stage { flex: 1; min-height: 0; display: flex; flex-direction: column; }
  /* Room for the floating header above. */
  .stage > :global(.stage) { padding-top: 60px; }
  .typing { height: 20px; padding: 0 24px; font-size: 12px; }
</style>
