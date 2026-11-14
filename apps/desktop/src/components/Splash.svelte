<script lang="ts">
  import { fade } from 'svelte/transition'
  import Icon from './Icon.svelte'
</script>

<!-- Covers the app while it loads; fades out the moment it is ready. -->
<div class="splash" out:fade={{ duration: 200 }} role="status" aria-label="Loading Rynuvo">
  <div class="logo"><Icon name="logo" size={56} /></div>
  <div class="word">Rynuvo</div>
</div>

<style>
  .splash {
    position: fixed; inset: 0; z-index: 1000; background: var(--bg);
    display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 14px;
  }
  .logo { position: relative; color: var(--accent); display: grid; place-items: center; }
  .logo::before {
    content: ''; position: absolute; width: 120px; height: 120px; border-radius: 50%; z-index: -1;
    background: radial-gradient(closest-side, var(--accent-soft), transparent);
    animation: breathe 2.4s ease-in-out infinite;
  }
  .word { font-size: 20px; font-weight: 700; letter-spacing: 0.02em; color: var(--text); }
  @keyframes breathe {
    0%, 100% { transform: scale(0.85); opacity: 0.6; }
    50% { transform: scale(1.15); opacity: 1; }
  }
  @media (prefers-reduced-motion: reduce) {
    .logo::before { animation: none; }
  }
</style>
