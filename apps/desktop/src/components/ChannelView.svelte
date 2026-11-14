<script lang="ts">
  import Composer from './Composer.svelte'
  import MessageList from './MessageList.svelte'
  import { store } from '../lib/state.svelte'
  import type { Channel } from '../lib/types'

  let { channel }: { channel: Channel } = $props()

  let now = $state(Date.now())
  $effect(() => {
    const t = setInterval(() => (now = Date.now()), 1000)
    return () => clearInterval(t)
  })
  const typers = $derived(store.typingNames(channel.id, now))
</script>

<header class="top">
  <span class="ico">{channel.kind === 'text' ? '#' : '🔊'}</span>
  <span class="name">{channel.name}</span>
  {#if channel.topic}<span class="topic">{channel.topic}</span>{/if}
</header>

{#if channel.kind === 'voice'}
  <section class="soon muted">
    <div class="big">🔊</div>
    <p>Voice segera hadir di update berikutnya.</p>
  </section>
{:else}
  <MessageList channel={channel.id} />
  <div class="typing muted">
    {#if typers.length === 1}{typers[0]} sedang mengetik…{:else if typers.length > 1}{typers.join(', ')} sedang mengetik…{/if}
  </div>
  <Composer {channel} />
{/if}

<style>
  .top { display: flex; align-items: center; gap: 8px; padding: 12px 18px; border-bottom: 1px solid var(--line); min-height: 49px; }
  .ico { color: var(--faint); }
  .name { font-weight: 600; }
  .topic { color: var(--muted); font-size: 13px; border-left: 1px solid var(--line); padding-left: 10px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .soon { flex: 1; display: grid; place-content: center; text-align: center; }
  .big { font-size: 40px; }
  .typing { height: 20px; padding: 0 20px; font-size: 12px; }
</style>
