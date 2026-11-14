<script lang="ts">
  import Icon from './Icon.svelte'
  import Composer from './Composer.svelte'
  import MessageList from './MessageList.svelte'
  import { store } from '../lib/state.svelte'
  import type { Channel, Id } from '../lib/types'
  import VoiceStage from './VoiceStage.svelte'
  import MemberList from './MemberList.svelte'
  import { slide } from 'svelte/transition'

  let { channel, onMemberMenu }: { channel: Channel; onMemberMenu: (userId: Id, e: MouseEvent) => void } = $props()

  let now = $state(Date.now())
  $effect(() => {
    const t = setInterval(() => (now = Date.now()), 1000)
    return () => clearInterval(t)
  })
  const typers = $derived(store.typingNames(channel.id, now))

  // Member list on the right in text channels; the choice is remembered on this device.
  let showMembers = $state(load())
  function load(): boolean {
    try {
      return localStorage.getItem('rynuvo.members') !== 'hidden'
    } catch {
      return true
    }
  }
  function toggleMembers() {
    showMembers = !showMembers
    try {
      localStorage.setItem('rynuvo.members', showMembers ? 'shown' : 'hidden')
    } catch {
      // not remembered; fine
    }
  }
</script>

<header class="top">
  <span class="ico"><Icon name={channel.kind === 'text' ? 'hash' : 'volume'} size={20} /></span>
  <span class="name">{channel.name}</span>
  {#if channel.topic}<span class="topic">{channel.topic}</span>{/if}
  {#if channel.kind === 'text'}
    <button class="tool" class:active={showMembers} title={showMembers ? 'Hide members' : 'Show members'} aria-pressed={showMembers} onclick={toggleMembers}>
      <Icon name="users" size={18} />
    </button>
  {/if}
</header>

{#if channel.kind === 'voice'}
  <VoiceStage {channel} onMenu={onMemberMenu} />
{:else}
  <div class="body">
    <div class="chat">
      <MessageList channel={channel.id} />
      <div class="typing muted">
        {#if typers.length === 1}{typers[0]} is typing…{:else if typers.length > 1}{typers.join(', ')} are typing…{/if}
      </div>
      <Composer {channel} />
    </div>
    {#if showMembers}<div class="side" transition:slide={{ axis: 'x', duration: 180 }}><MemberList onMenu={onMemberMenu} /></div>{/if}
  </div>
{/if}

<style>
  .top { display: flex; align-items: center; gap: 8px; padding: 12px 18px; border-bottom: 1px solid var(--line); min-height: 49px; }
  .ico { color: var(--faint); display: inline-flex; }
  .name { font-weight: 600; }
  .topic { color: var(--muted); font-size: 13px; border-left: 1px solid var(--line); padding-left: 10px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .tool { margin-left: auto; display: grid; place-items: center; padding: 5px; border-radius: 6px; color: var(--muted); }
  .tool:hover { color: var(--text); background: var(--elev-2); }
  .tool.active { color: var(--text); }
  .body { flex: 1; display: flex; min-height: 0; }
  .chat { flex: 1; min-width: 0; display: flex; flex-direction: column; }
  .side { display: flex; overflow: hidden; }
  .typing { height: 20px; padding: 0 20px; font-size: 12px; }
</style>
