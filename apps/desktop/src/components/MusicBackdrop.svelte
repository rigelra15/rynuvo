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
</div>

<style>
  .wrap { position: absolute; inset: 0; z-index: -1; overflow: hidden; pointer-events: none; }
  .art {
    position: absolute; inset: -20%; background-size: cover; background-position: center;
    filter: blur(60px) saturate(1.4); transform: scale(1.2); animation: drift 18s ease-in-out infinite alternate;
  }
  .shade { position: absolute; inset: 0; background: linear-gradient(180deg, rgba(10, 10, 16, 0.35), rgba(10, 10, 16, 0.78)); }
  @keyframes drift { to { transform: scale(1.32) rotate(12deg); } }
  @media (prefers-reduced-motion: reduce) { .art { animation: none; } }
</style>
