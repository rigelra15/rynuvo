<script lang="ts">
  import { onMount } from 'svelte'
  import { fade, fly } from 'svelte/transition'
  import { api, errText } from '../lib/api'
  import { pane } from '../lib/pane.svelte'
  import { phonePortal } from '../lib/portal'
  import { sheet } from '../lib/motion'
  import { isAndroid } from '../lib/platform'
  import Icon from './Icon.svelte'

  // Switch mic and speaker mid-call, without going to Settings: above the call dock, or a sheet from
  // the bottom on a phone. The same choice Settings → Voice saves.
  let { onClose }: { onClose: () => void } = $props()

  let inputs = $state<string[]>([])
  let outputs = $state<string[]>([])
  let input = $state('')
  let output = $state('')
  let loaded = $state(false)
  let error = $state<string | null>(null)
  let box = $state<HTMLElement>()

  onMount(() => {
    void (async () => {
      try {
        const [p, d] = await Promise.all([api.prefsGet(), api.voiceDevices()])
        input = p.inputDevice ?? ''
        output = p.outputDevice ?? ''
        // Android also lists the phone's own odd parts (bare model name, virtual ones); only what
        // you'd talk or listen through.
        const all = [...d.inputs, ...d.outputs]
        const model = new Set(all.map((n) => /^(.*) \((Speaker|Earpiece|Microphone)\)$/.exec(n)?.[1]).filter(Boolean))
        const keep = (n: string, i: number, list: string[]) =>
          list.indexOf(n) === i && (!isAndroid || (!model.has(n) && !/\((Virtual|Tuner)\)$/.test(n)))
        inputs = d.inputs.filter(keep)
        outputs = d.outputs.filter(keep)
      } catch (e) {
        error = errText(e)
      }
      loaded = true
    })()
    const key = (e: KeyboardEvent) => e.key === 'Escape' && onClose()
    const away = (e: PointerEvent) => {
      const t = e.target as Element | null
      if (box && !box.contains(t) && !t?.closest('[data-audio-toggle]')) onClose()
    }
    window.addEventListener('keydown', key)
    window.addEventListener('pointerdown', away, true)
    // Android's Back closes the sheet first (MainActivity asks the page before going back).
    const w = window as unknown as { __rynuvoBack?: () => boolean }
    const back = () => (onClose(), true)
    w.__rynuvoBack = back
    return () => {
      if (w.__rynuvoBack === back) delete w.__rynuvoBack
      window.removeEventListener('keydown', key)
      window.removeEventListener('pointerdown', away, true)
    }
  })

  /** Android names built-in parts after the phone's model ("SM-A566E (Speaker)"): say what they are. */
  function label(n: string): string {
    if (!isAndroid) return n
    const m = /^(.*) \((Speaker|Earpiece|Microphone)\)$/.exec(n)
    if (!m) return n.replace(/ \((Headset|Headphones)\)$/, ' · $1')
    return { Speaker: 'Phone speaker', Earpiece: 'Phone earpiece', Microphone: 'Phone mic' }[m[2]] ?? n
  }

  function pick(which: 'in' | 'out', v: string) {
    if (which === 'in') input = v
    else output = v
    error = null
    void api.voiceSetDevices(input || null, output || null).catch((e) => (error = errText(e)))
  }
  const motion = (node: Element) => (pane.narrow ? sheet(node, {}, { direction: 'both' }) : fly(node, { y: 8, duration: 200 }))
</script>

{#snippet list(which: 'in' | 'out', names: string[], current: string)}
  <ul>
    {#each ['', ...names] as n (n)}
      <li>
        <button class:on={current === n} onclick={() => pick(which, n)} aria-pressed={current === n}>
          <span class="nm">{n ? label(n) : 'System default'}</span>
          {#if current === n}<span class="tick" in:fly={{ y: 3, duration: 160 }}><Icon name="check" size={15} /></span>{/if}
        </button>
      </li>
    {/each}
  </ul>
{/snippet}

{#if pane.narrow}<div class="scrim" role="presentation" use:phonePortal transition:fade={{ duration: 200 }}></div>{/if}
<div class="pick" bind:this={box} class:phone={pane.narrow} role="dialog" aria-label="Sound devices" use:phonePortal transition:motion>
  {#if pane.narrow}<span class="grab" aria-hidden="true"></span>{/if}
  <div class="sec"><Icon name="mic" size={14} /> Microphone</div>
  {#if loaded}{@render list('in', inputs, input)}{:else}<p class="muted">Looking…</p>{/if}
  <div class="sec"><Icon name="headphones" size={14} /> Speaker</div>
  {#if loaded}{@render list('out', outputs, output)}{/if}
  {#if error}<p class="err">{error}</p>{/if}
</div>

<style>
  .scrim { position: fixed; inset: 0; z-index: 60; background: rgba(8, 6, 12, 0.55); }
  .pick {
    position: absolute; z-index: 61; bottom: calc(100% + 12px); left: 50%; translate: -50% 0; width: min(340px, calc(100vw - 32px));
    max-height: min(460px, 65vh); overflow-y: auto; display: flex; flex-direction: column; padding: 12px 10px 10px; border-radius: 18px;
    background: var(--elev); border: 1px solid var(--line); box-shadow: 0 18px 50px rgba(0, 0, 0, 0.5); color: var(--text);
  }
  .pick.phone {
    position: fixed; left: 0; right: 0; bottom: 0; translate: none; width: auto; max-height: 75vh; border-radius: 22px 22px 0 0;
    padding: 8px 14px calc(env(safe-area-inset-bottom) + 16px);
  }
  .grab { align-self: center; width: 40px; height: 4px; border-radius: 2px; background: var(--line); margin-bottom: 10px; }
  .sec { display: flex; align-items: center; gap: 7px; padding: 8px 8px 4px; font-size: 12px; font-weight: 700; letter-spacing: 0.04em; text-transform: uppercase; color: var(--muted); }
  ul { list-style: none; margin: 0 0 4px; padding: 0; }
  li button { width: 100%; display: flex; align-items: center; gap: 10px; padding: 9px 10px; border-radius: 11px; text-align: left; font-size: 14px; }
  li button:hover { background: var(--panel-2); }
  li button.on { background: var(--accent-soft); color: var(--text); font-weight: 650; }
  .nm { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .tick { display: inline-flex; color: var(--accent); }
  .muted { margin: 4px 10px; font-size: 13px; color: var(--muted); }
  .err { margin: 6px 10px 2px; font-size: 13px; color: var(--danger); }
  @media (pointer: coarse) { li button { padding: 12px 10px; } }
</style>
