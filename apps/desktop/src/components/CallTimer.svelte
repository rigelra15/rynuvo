<script lang="ts">
  // How long the call in a voice channel has been going ("12:34", "1:02:03").
  import { store } from '../lib/state.svelte'
  import { membersOf } from '../lib/voice'
  import { callStart } from '../lib/call'
  import { clock } from '../lib/music'
  import type { Id } from '../lib/types'

  let { channelId, prefix = '' }: { channelId: Id; prefix?: string } = $props()
  const start = $derived(callStart(membersOf(store.voice, channelId)))
  let now = $state(Date.now())
  $effect(() => {
    if (start === null) return
    const iv = setInterval(() => (now = Date.now()), 1000)
    return () => clearInterval(iv)
  })
</script>

{#if start !== null}{prefix}<span class="timer" title="How long this call has been going">{clock(Math.max(0, now - start))}</span>{/if}

<style>
  .timer { font-variant-numeric: tabular-nums; }
</style>
