<script lang="ts">
  import { tick } from 'svelte'
  import { openUrl } from '@tauri-apps/plugin-opener'
  import MessageView from './Message.svelte'
  import { store } from '../lib/state.svelte'
  import type { Id } from '../lib/types'
  import { dayLabel, isHead, newDay } from '../lib/chat'

  let { channel }: { channel: Id } = $props()

  let el: HTMLDivElement | undefined = $state()
  let stick = true
  let loadingOlder = false

  const list = $derived(store.messages.get(channel))
  const now = $derived(list ? Date.now() : 0)

  // keep pinned to bottom on new messages unless the user scrolled up
  $effect.pre(() => {
    void list?.length
    if (!el) return
    stick = el.scrollHeight - el.scrollTop - el.clientHeight < 80
    tick().then(() => {
      if (stick && el) el.scrollTop = el.scrollHeight
    })
  })

  // reset to bottom when switching channel
  $effect(() => {
    void channel
    stick = true
    tick().then(() => el && (el.scrollTop = el.scrollHeight))
  })

  async function onScroll() {
    if (!el || loadingOlder || el.scrollTop > 60) return
    loadingOlder = true
    const prevHeight = el.scrollHeight
    try {
      const n = await store.loadOlder(channel)
      if (n > 0) {
        await tick()
        el.scrollTop = el.scrollHeight - prevHeight
      }
    } finally {
      loadingOlder = false
    }
  }

  function onClick(e: MouseEvent) {
    const a = (e.target as HTMLElement).closest('a')
    if (a?.href) {
      e.preventDefault()
      void openUrl(a.href)
    }
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="list" bind:this={el} onscroll={onScroll} onclick={onClick}>
  {#if list === undefined && store.loadError.has(channel)}
    <div class="empty muted">
      <p>Couldn't load messages: {store.loadError.get(channel)}</p>
      <button class="btn sm" onclick={() => store.loadLatest(channel)}>Retry</button>
    </div>
  {:else if list === undefined}
    <div class="empty muted">Loading…</div>
  {:else if list.length === 0}
    <div class="empty muted">No messages yet. Say hi to your friends!</div>
  {:else}
    {#if store.hasMore.get(channel) === false}<div class="start muted">This is the start of the channel</div>{/if}
    {#each list as m, i (m.id)}
      {@const prev = list[i - 1]}
      {#if newDay(prev, m)}<div class="day"><span>{dayLabel(m.createdAt, now)}</span></div>{/if}
      <MessageView {m} head={isHead(prev, m) || newDay(prev, m)} />
    {/each}
  {/if}
</div>

<style>
  .list { flex: 1; overflow-y: auto; padding-bottom: 8px; }
  .empty { height: 100%; display: grid; place-items: center; }
  .start { padding: 16px 18px 0; font-size: 12px; text-align: center; }
  .day { display: flex; justify-content: center; margin: 14px 0 4px; position: sticky; top: 6px; z-index: 1; pointer-events: none; }
  .day span { font-size: 11.5px; font-weight: 600; color: var(--muted); background: var(--elev-2); border: 1px solid var(--line); padding: 3px 10px; border-radius: 999px; }
</style>
