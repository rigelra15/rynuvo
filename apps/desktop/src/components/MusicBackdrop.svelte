<script lang="ts">
  // The album cover, blurred and darkened behind the stage; a new cover fades in over the old one.
  import { fade } from 'svelte/transition'
  let { url }: { url: string } = $props()
</script>

<div class="wrap" aria-hidden="true">
  {#key url}
    <div class="art" style="background-image:url('{url}')" transition:fade={{ duration: 900 }}></div>
  {/key}
  <div class="shade"></div>
  <div class="breath"></div>
</div>

<style>
  .wrap { position: absolute; inset: 0; z-index: -1; overflow: hidden; pointer-events: none; }
  .art {
    position: absolute; inset: -20%; background-size: cover; background-position: center;
    filter: blur(60px) saturate(1.4); transform: scale(1.2); animation: drift 18s ease-in-out infinite alternate;
  }
  .shade { position: absolute; inset: 0; background: linear-gradient(180deg, rgba(10, 10, 16, 0.35), rgba(10, 10, 16, 0.78)); }
  @keyframes drift { to { transform: scale(1.32) rotate(12deg); } }
  /* A slow breath of light over the cover colours: brighter and wider, then calm again. */
  .breath {
    position: absolute; inset: -10%; mix-blend-mode: soft-light;
    background: radial-gradient(60% 55% at 50% 45%, rgba(255, 255, 255, 0.55), transparent 70%);
    animation: breathe 7s ease-in-out infinite;
  }
  @keyframes breathe {
    0%, 100% { opacity: 0.15; transform: scale(0.92); }
    50% { opacity: 0.75; transform: scale(1.08); }
  }
  @media (prefers-reduced-motion: reduce) { .art, .breath { animation: none; } }
</style>
