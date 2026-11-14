<script lang="ts">
  let { name, size = 18 }: { name: 'mic' | 'micOff' | 'headphones' | 'headphonesOff' | 'screen' | 'leave'; size?: number } = $props()
</script>

<svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
  {#if name === 'mic' || name === 'micOff'}
    <rect x="9" y="3" width="6" height="11" rx="3" />
    <path d="M5 11a7 7 0 0 0 14 0M12 18v3" />
  {:else if name === 'headphones' || name === 'headphonesOff'}
    <path d="M4 15v-3a8 8 0 0 1 16 0v3" />
    <rect x="3" y="14" width="4" height="6" rx="1.5" />
    <rect x="17" y="14" width="4" height="6" rx="1.5" />
  {:else if name === 'screen'}
    <rect x="3" y="4" width="18" height="12" rx="2" />
    <path d="M8 20h8M12 16v4" />
  {:else if name === 'leave'}
    <path d="M4.5 14.5c4.5-4 10.5-4 15 0l-2 2.5-3-1.2v-2.3a10 10 0 0 0-5 0v2.3l-3 1.2z" />
  {/if}
  {#if name === 'micOff' || name === 'headphonesOff'}
    <path d="M3 3l18 18" stroke="var(--danger)" />
  {/if}
</svg>
