<script lang="ts">
  import { fly } from 'svelte/transition'
  import { captions } from '../lib/captions.svelte'
  import { store } from '../lib/state.svelte'

  // The latest caption lines, at the foot of the room (or over a stream shown full screen).
  let { over = false }: { over?: boolean } = $props()
</script>

{#if captions.status === 'on' && captions.lines.length}
  <div class="cc" class:over aria-live="polite">
    {#each captions.lines as l (l.n)}
      <p in:fly={{ y: 8, duration: 220 }}>
        {#if l.speaker !== null}<b>{store.nick(l.speaker)}</b>{/if}{l.text}
      </p>
    {/each}
  </div>
{/if}

<style>
  .cc { display: flex; flex-direction: column; align-items: center; gap: 4px; pointer-events: none; max-width: min(720px, 92%); margin: 0 auto; }
  p {
    margin: 0; padding: 5px 12px; border-radius: 10px; font-size: 15px; line-height: 1.4; text-align: center; color: #fff;
    background: rgba(8, 6, 12, 0.78); box-decoration-break: clone; -webkit-box-decoration-break: clone;
  }
  p:not(:last-child) { opacity: 0.6; font-size: 14px; }
  b { color: var(--accent); font-weight: 700; margin-right: 6px; }
  .over { position: absolute; left: 0; right: 0; bottom: calc(env(safe-area-inset-bottom) + 96px); z-index: 2; margin: 0 auto; }
  .over p { font-size: 17px; }
  @media (min-width: 900px) { .over p { font-size: 20px; } }
</style>
