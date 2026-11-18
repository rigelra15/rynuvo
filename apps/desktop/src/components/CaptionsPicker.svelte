<script lang="ts">
  import { onMount } from 'svelte'
  import { fade, fly } from 'svelte/transition'
  import { captions, type CaptionLang } from '../lib/captions.svelte'
  import { pane } from '../lib/pane.svelte'
  import { phonePortal } from '../lib/portal'
  import { sheet } from '../lib/motion'

  // Captions on or off, and in which language. Above the call dock, or a sheet on a phone.
  let { onClose }: { onClose: () => void } = $props()
  let box = $state<HTMLElement>()

  onMount(() => {
    void captions.init()
    const key = (e: KeyboardEvent) => e.key === 'Escape' && onClose()
    const away = (e: PointerEvent) => {
      const t = e.target as Element | null
      if (box && !box.contains(t) && !t?.closest('[data-cc-toggle]')) onClose()
    }
    window.addEventListener('keydown', key)
    window.addEventListener('pointerdown', away, true)
    const w = window as unknown as { __rynuvoBack?: () => boolean }
    const back = () => (onClose(), true)
    w.__rynuvoBack = back
    return () => {
      if (w.__rynuvoBack === back) delete w.__rynuvoBack
      window.removeEventListener('keydown', key)
      window.removeEventListener('pointerdown', away, true)
    }
  })

  const LANGS: { id: CaptionLang; label: string; hint: string }[] = [
    { id: 'id', label: 'Indonesia', hint: 'Tulis apa yang diucapkan dalam Bahasa Indonesia' },
    { id: 'en', label: 'English', hint: 'English, translating whatever is spoken' },
  ]
  const pct = $derived(Math.round(captions.progress * 100))
  const motion = (node: Element) => (pane.narrow ? sheet(node, {}, { direction: 'both' }) : fly(node, { y: 8, duration: 200 }))
</script>

{#if pane.narrow}<div class="scrim" role="presentation" use:phonePortal transition:fade={{ duration: 200 }}></div>{/if}
<div class="pick" bind:this={box} class:phone={pane.narrow} role="dialog" aria-label="Captions" use:phonePortal transition:motion>
  {#if pane.narrow}<span class="grab" aria-hidden="true"></span>{/if}
  <div class="head">
    <span><b>Captions</b><span class="sub">What's said in the call, as text. Made on this device; nothing leaves it.</span></span>
    <button class="switch" class:on={captions.wanted} role="switch" aria-checked={captions.wanted} aria-label="Captions" onclick={() => void captions.toggle()}><i></i></button>
  </div>
  <div class="langs" role="radiogroup" aria-label="Caption language">
    {#each LANGS as l (l.id)}
      <button role="radio" aria-checked={captions.lang === l.id} class:on={captions.lang === l.id} onclick={() => void captions.setLang(l.id)}>
        <span class="lb">{l.label}</span><span class="hint">{l.hint}</span>
      </button>
    {/each}
  </div>
  {#if captions.status === 'downloading'}
    <div class="state">
      <span>Downloading the captions model (once, about 190 MB)… {pct}%</span>
      <div class="bar"><i style="width:{pct}%"></i></div>
    </div>
  {:else if captions.status === 'loading'}
    <p class="state">Getting ready…</p>
  {:else if captions.status === 'on'}
    <p class="state ok">On. Lines show up a moment after they're said.</p>
  {:else if captions.status === 'error'}
    <p class="state err">{captions.error}</p>
  {/if}
</div>

<style>
  .scrim { position: fixed; inset: 0; z-index: 60; background: rgba(8, 6, 12, 0.55); }
  .pick {
    position: absolute; z-index: 61; bottom: calc(100% + 12px); left: 50%; translate: -50% 0; width: min(360px, calc(100vw - 32px));
    display: flex; flex-direction: column; gap: 12px; padding: 14px; border-radius: 18px;
    background: var(--elev); border: 1px solid var(--line); box-shadow: 0 18px 50px rgba(0, 0, 0, 0.5); color: var(--text);
  }
  .pick.phone {
    position: fixed; left: 0; right: 0; bottom: 0; translate: none; width: auto; border-radius: 22px 22px 0 0;
    padding: 8px 18px calc(env(safe-area-inset-bottom) + 18px);
  }
  .grab { align-self: center; width: 40px; height: 4px; border-radius: 2px; background: var(--line); }
  .head { display: flex; align-items: center; gap: 14px; }
  .head > span { flex: 1; display: flex; flex-direction: column; gap: 3px; }
  .head b { font-size: 15px; }
  .sub { font-size: 12.5px; color: var(--muted); line-height: 1.4; }
  .switch { flex: none; width: 46px; height: 28px; border-radius: 999px; background: var(--panel-2); border: 1px solid var(--line); position: relative; padding: 0; transition: background 200ms; }
  .switch i { position: absolute; top: 3px; left: 3px; width: 20px; height: 20px; border-radius: 50%; background: var(--muted); transition: transform 220ms cubic-bezier(0.22, 1, 0.36, 1), background 200ms; }
  .switch.on { background: var(--accent); border-color: transparent; }
  .switch.on i { transform: translateX(18px); background: var(--on-accent); }
  .langs { display: grid; grid-template-columns: 1fr 1fr; gap: 8px; }
  .langs button { display: flex; flex-direction: column; align-items: flex-start; gap: 3px; padding: 10px 12px; border-radius: 12px; text-align: left; border: 1px solid var(--line); background: var(--panel-2); }
  .langs button.on { border-color: var(--accent); background: var(--accent-soft); }
  .lb { font-weight: 700; font-size: 14px; }
  .hint { font-size: 11.5px; color: var(--muted); line-height: 1.35; }
  .state { margin: 0; font-size: 13px; color: var(--muted); display: flex; flex-direction: column; gap: 6px; }
  .state.ok { color: var(--speaking); }
  .state.err { color: var(--danger); }
  .bar { height: 6px; border-radius: 3px; background: var(--panel-2); overflow: hidden; }
  .bar i { display: block; height: 100%; background: var(--accent); transition: width 200ms; }
</style>
