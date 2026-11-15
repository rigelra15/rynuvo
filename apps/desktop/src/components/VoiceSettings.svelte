<script lang="ts">
  import Slider from './Slider.svelte'
  import { onDestroy, onMount } from 'svelte'
  import { api, errText } from '../lib/api'
  import { store } from '../lib/state.svelte'
  import type { Prefs } from '../lib/voice'
  import ShortcutInput from './ShortcutInput.svelte'

  let inputs = $state<string[]>([])
  let outputs = $state<string[]>([])
  let input = $state('')
  let output = $state('')
  let sensitivity = $state(50)
  let testing = $state(false)
  let error = $state<string | null>(null)

  let prefs = $state<Prefs | null>(null)
  let scMsg = $state<string | null>(null)

  onMount(async () => {
    try {
      prefs = await api.prefsGet()
      input = prefs.inputDevice ?? ''
      output = prefs.outputDevice ?? ''
      sensitivity = Math.round(prefs.vadThreshold * 100)
      const d = await api.voiceDevices()
      inputs = d.inputs
      outputs = d.outputs
    } catch (e) {
      error = errText(e)
    }
  })
  onDestroy(() => {
    if (testing) void api.voiceMicTest(false)
  })

  const meter = $derived(Math.max(0, Math.min(100, ((store.micLevel.db + 60) / 60) * 100)))

  function devices() {
    void api.voiceSetDevices(input || null, output || null).catch((e) => (error = errText(e)))
  }
  function threshold() {
    void api.voiceSetThreshold(sensitivity / 100)
  }
  async function shortcut(which: 'shortcutMute' | 'shortcutDeafen', v: string) {
    if (!prefs) return
    try {
      prefs = await api.prefsSet({ ...prefs, [which]: v })
      scMsg = null
    } catch (e) {
      scMsg = errText(e)
    }
  }

  async function test() {
    testing = !testing
    await api.voiceMicTest(testing)
    if (!testing && store.myVoice.channelId === null) store.micLevel = { db: -100, open: false }
  }
</script>

<section>
  <h3>Input device</h3>
  <select bind:value={input} onchange={devices}>
    <option value="">System default</option>
    {#each inputs as d}<option value={d}>{d}</option>{/each}
  </select>
</section>

<section>
  <h3>Output device</h3>
  <select bind:value={output} onchange={devices}>
    <option value="">System default</option>
    {#each outputs as d}<option value={d}>{d}</option>{/each}
  </select>
</section>

<section>
  <h3>Mic threshold</h3>
  <p class="muted">Your mic opens when the bar turns green. Raise this if background noise gets through.</p>
  <div class="meter"><div class="fill" class:open={store.micLevel.open} style="width:{meter}%"></div></div>
  <Slider bind:value={sensitivity} max={100} step={1} label="Mic threshold" oninput={threshold} />
  <button class="btn" onclick={test}>{testing ? 'Stop test' : 'Test mic'}</button>
  {#if testing}<p class="muted">You should hear yourself. Use headphones to avoid echo.</p>{/if}
</section>

{#if prefs}
  <section>
    <h3>Shortcuts</h3>
    <p class="muted">Work even when Rynuvo is in the background.</p>
    <div class="sc"><span>Mute</span><ShortcutInput value={prefs.shortcutMute} onChange={(v) => shortcut('shortcutMute', v)} /></div>
    <div class="sc"><span>Deafen</span><ShortcutInput value={prefs.shortcutDeafen} onChange={(v) => shortcut('shortcutDeafen', v)} /></div>
    {#if scMsg}<p class="err">{scMsg}</p>{/if}
  </section>
{/if}

{#if error}<p class="err">{error}</p>{/if}

<style>
  section { padding: 14px 0; border-bottom: 1px solid var(--line); display: flex; flex-direction: column; gap: 8px; align-items: flex-start; }
  h3 { margin: 0; font-size: 13px; text-transform: uppercase; letter-spacing: 0.04em; color: var(--muted); }
  p { margin: 0; font-size: 13px; }
  select { width: 100%; }
  .meter { width: 100%; height: 8px; border-radius: 4px; background: var(--elev); overflow: hidden; }
  .fill { height: 100%; background: var(--faint); transition: width 90ms linear; }
  .fill.open { background: var(--speaking); }
  .err { color: var(--danger); }
  .sc { display: flex; justify-content: space-between; align-items: center; width: 100%; }
</style>
