<script lang="ts">
  import Icon from './Icon.svelte'
  import { store } from '../lib/state.svelte'

  let { circleName = 'Rynuvo' }: { circleName?: string } = $props()
  const inVoice = $derived(store.myVoice.channelId !== null)
</script>

<!-- The far-left strip: Home, then the circle. -->
<nav class="rail" aria-label="Places">
  <button class="item" class:on={store.view === 'home'} title="Home" aria-current={store.view === 'home'} onclick={() => (store.view = 'home')}>
    <span class="pill"></span>
    <span class="tile home"><Icon name="home" size={22} /></span>
  </button>
  <span class="sep"></span>
  <button class="item" class:on={store.view === 'circle'} title={circleName} aria-current={store.view === 'circle'} onclick={() => (store.view = 'circle')}>
    <span class="pill"></span>
    <span class="tile circle"><Icon name="logo" size={22} /></span>
    {#if inVoice}<span class="badge" title="You're in voice"><Icon name="volume" size={10} /></span>{/if}
  </button>
</nav>

<style>
  .rail { width: 64px; flex-shrink: 0; background: var(--panel-2); display: flex; flex-direction: column; align-items: center; gap: 8px; padding: 12px 0; }
  .item { position: relative; display: grid; place-items: center; width: 64px; }
  .tile {
    width: 44px; height: 44px; border-radius: 50%; display: grid; place-items: center; background: var(--elev-2); color: var(--muted);
    transition: border-radius 180ms ease, background 180ms ease, color 180ms ease;
  }
  .item:hover .tile, .item.on .tile { border-radius: 14px; }
  .item:hover .tile { background: var(--elev); color: var(--text); }
  .item.on .tile.home { background: var(--accent); color: #fff; }
  .item.on .tile.circle { background: var(--elev); color: var(--accent); }
  .pill {
    position: absolute; left: 0; width: 4px; height: 8px; border-radius: 0 4px 4px 0; background: #fff;
    transform: scaleY(0); transition: transform 180ms ease, height 180ms ease;
  }
  .item:hover .pill { transform: scaleY(1); height: 20px; }
  .item.on .pill { transform: scaleY(1); height: 36px; }
  .sep { width: 32px; height: 2px; border-radius: 1px; background: var(--line); }
  .badge { position: absolute; right: 8px; bottom: -2px; width: 18px; height: 18px; border-radius: 50%; background: var(--speaking); color: #0b2a14; display: grid; place-items: center; border: 3px solid var(--panel-2); box-sizing: content-box; }
  @media (prefers-reduced-motion: reduce) {
    .tile, .pill { transition: none; }
  }
</style>
