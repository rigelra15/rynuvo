<script lang="ts" module>
  export type IconName =
    | 'mic' | 'micOff' | 'headphones' | 'headphonesOff' | 'screen' | 'leave'
    | 'hash' | 'volume' | 'plus' | 'settings' | 'edit' | 'trash' | 'close' | 'check' | 'logo' | 'users' | 'home'
    | 'play' | 'pause' | 'skip' | 'queue' | 'music' | 'repeat' | 'repeatOne'
</script>

<script lang="ts">
  /** The app's only icon set: 24×24 outline, 2px stroke, round joins. */
  let { name, size = 18 }: { name: IconName; size?: number } = $props()
</script>

<svg class="icon" width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
  {#if name === 'mic' || name === 'micOff'}
    <rect x="9" y="3" width="6" height="11" rx="3" />
    <path d="M5 11a7 7 0 0 0 14 0M12 18v3" />
  {:else if name === 'headphones' || name === 'headphonesOff'}
    <path d="M4 15v-3a8 8 0 0 1 16 0v3" />
    <rect x="3" y="14" width="4" height="6" rx="1.5" />
    <rect x="17" y="14" width="4" height="6" rx="1.5" />
  {:else if name === 'play'}
    <path d="M7 4.5v15l12.5-7.5z" />
  {:else if name === 'pause'}
    <path d="M8 5v14M16 5v14" />
  {:else if name === 'skip'}
    <path d="M5 5l10 7-10 7zM19 5v14" />
  {:else if name === 'repeat' || name === 'repeatOne'}
    <path d="M17 2l3 3-3 3" /><path d="M4 11V9a4 4 0 0 1 4-4h12" /><path d="M7 22l-3-3 3-3" /><path d="M20 13v2a4 4 0 0 1-4 4H4" />
    {#if name === 'repeatOne'}<path d="M11 10h1v4" />{/if}
  {:else if name === 'queue'}
    <path d="M4 6h16M4 12h10M4 18h10M18 14v6l4-3z" />
  {:else if name === 'music'}
    <path d="M9 18V5l12-2v13" /><circle cx="6" cy="18" r="3" /><circle cx="18" cy="16" r="3" />
  {:else if name === 'screen'}
    <rect x="3" y="4" width="18" height="12" rx="2" />
    <path d="M8 20h8M12 16v4" />
  {:else if name === 'leave'}
    <path d="M4.5 14.5c4.5-4 10.5-4 15 0l-2 2.5-3-1.2v-2.3a10 10 0 0 0-5 0v2.3l-3 1.2z" />
  {:else if name === 'home'}
    <path d="M15 21v-8a1 1 0 0 0-1-1h-4a1 1 0 0 0-1 1v8" /><path d="M3 10a2 2 0 0 1 .709-1.528l7-5.999a2 2 0 0 1 2.582 0l7 5.999A2 2 0 0 1 21 10v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" />
  {:else if name === 'users'}
    <path d="M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2" /><circle cx="9" cy="7" r="4" /><path d="M22 21v-2a4 4 0 0 0-3-3.87M16 3.13a4 4 0 0 1 0 7.75" />
  {:else if name === 'hash'}
    <path d="M5 9h15M4 15h15M10 3 8 21M16 3l-2 18" />
  {:else if name === 'volume'}
    <path d="M11 5 6 9H3v6h3l5 4z" />
    <path d="M15.5 8.5a5 5 0 0 1 0 7M18.5 5.5a9 9 0 0 1 0 13" />
  {:else if name === 'plus'}
    <path d="M12 5v14M5 12h14" />
  {:else if name === 'settings'}
    <circle cx="12" cy="12" r="3" />
    <path d="M19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8V9a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1z" />
  {:else if name === 'edit'}
    <path d="M12 20h9" />
    <path d="M16.5 3.5a2.1 2.1 0 0 1 3 3L7 19l-4 1 1-4z" />
  {:else if name === 'trash'}
    <path d="M3 6h18M8 6V4h8v2M19 6l-1 14H6L5 6M10 11v6M14 11v6" />
  {:else if name === 'close'}
    <path d="M18 6 6 18M6 6l12 12" />
  {:else if name === 'check'}
    <path d="M20 6 9 17l-5-5" />
  {:else if name === 'logo'}
    <path d="M12 2.5 21.5 12 12 21.5 2.5 12z" fill="currentColor" stroke="none" />
  {/if}
  {#if name === 'micOff' || name === 'headphonesOff'}
    <path d="M3 3l18 18" stroke="var(--danger)" />
  {/if}
</svg>

<style>
  .icon { display: block; flex-shrink: 0; }
</style>
