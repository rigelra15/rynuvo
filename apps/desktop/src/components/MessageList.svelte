<script lang="ts">
  import { tick } from 'svelte'
  import { openUrl } from '@tauri-apps/plugin-opener'
  import MessageView from './Message.svelte'
  import { store } from '../lib/state.svelte'
  import type { Id } from '../lib/types'

  let { channel }: { channel: Id } = $props()

  let el: HTMLDivElement | undefined = $state()
  let stick = true
  let loadingOlder = false

  const list = $derived(store.messages.get(channel))
  const GROUP_MS = 5 * 60 * 1000

  function isHead(i: number): boolean {
    if (!list || i === 0) return true
    const a = list[i - 1]
    const b = list[i]
    return a.authorId !== b.authorId || b.createdAt - a.createdAt > GROUP_MS
  }

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
  {#if list === undefined}
    <div class="empty muted">Memuat…</div>
  {:else if list.length === 0}
    <div class="empty muted">No messages yet. Say hi to your friends!</div>
  {:else}
    {#if store.hasMore.get(channel) === false}<div class="start muted">This is the start of the channel</div>{/if}
    {#each list as m, i (m.id)}
      <MessageView {m} head={isHead(i)} />
    {/each}
  {/if}
</div>

<style>
  .list { flex: 1; overflow-y: auto; padding-bottom: 8px; }
  .empty { height: 100%; display: grid; place-items: center; }
  .start { padding: 16px 18px 0; font-size: 12px; }
</style>
