<script lang="ts">
  import Icon from './Icon.svelte'
  import Composer from './Composer.svelte'
  import MessageList from './MessageList.svelte'
  import { store } from '../lib/state.svelte'
  import type { Channel, Id } from '../lib/types'
  import VoiceStage from './VoiceStage.svelte'

  let { channel, onMemberMenu }: { channel: Channel; onMemberMenu: (userId: Id, e: MouseEvent) => void } = $props()

  let now = $state(Date.now())
  $effect(() => {
    const t = setInterval(() => (now = Date.now()), 1000)
    return () => clearInterval(t)
  })
  const typers = $derived(store.typingNames(channel.id, now))
</script>

<header class="top">
  <span class="ico"><Icon name={channel.kind === 'text' ? 'hash' : 'volume'} size={20} /></span>
  <span class="name">{channel.name}</span>
  {#if channel.topic}<span class="topic">{channel.topic}</span>{/if}
</header>

{#if channel.kind === 'voice'}
  <VoiceStage {channel} onMenu={onMemberMenu} />
{:else}
  <MessageList channel={channel.id} />
  <div class="typing muted">
    {#if typers.length === 1}{typers[0]} is typing…{:else if typers.length > 1}{typers.join(', ')} are typing…{/if}
  </div>
  <Composer {channel} />
{/if}

<style>
  .top { display: flex; align-items: center; gap: 8px; padding: 12px 18px; border-bottom: 1px solid var(--line); min-height: 49px; }
  .ico { color: var(--faint); display: inline-flex; }
  .name { font-weight: 600; }
  .topic { color: var(--muted); font-size: 13px; border-left: 1px solid var(--line); padding-left: 10px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .typing { height: 20px; padding: 0 20px; font-size: 12px; }
</style>
