<script lang="ts">
  import Slider from './Slider.svelte'
  import { onMount } from 'svelte'
  import { api, errText } from '../lib/api'
  import { CODEC_STRING, decodableCodecs, FramePacer, KeyGate, parseAu, RenderMeter, spsSize, statsLine, TsUnwrap, type CodecName, type RenderStats } from '../lib/stream'
  import { store } from '../lib/state.svelte'
  import type { Id } from '../lib/types'

  let { userId }: { userId: Id } = $props()

  let canvas: HTMLCanvasElement
  let wrap: HTMLDivElement
  let size = $state({ width: 0, height: 0 })
  let volume = $state(1)
  let waiting = $state(true)
  let render = $state<RenderStats | null>(null)
  const stats = $derived(store.streamStats.get(userId))
  // The fps shown is what reached the screen, not what arrived.
  const line = $derived(
    statsLine({ width: size.width, height: size.height, fps: render?.renderedFps ?? stats?.fps ?? 0, codec: stats?.codec ?? '', path: stats?.path ?? 'connecting' }),
  )
  const detail = $derived(
    render
      ? `Received ${render.receivedFps.toFixed(0)} fps · shown ${render.renderedFps.toFixed(0)} fps · frame gap ${render.intervalMs.toFixed(1)} ± ${render.jitterMs.toFixed(1)} ms · buffer ${render.bufferMs.toFixed(0)} ms · dropped ${render.lateDrops + render.gateDrops}`
      : '',
  )

  onMount(() => {
    const ctx = canvas.getContext('2d')!
    let decoder: VideoDecoder | null = null
    let codec: CodecName | null = null
    let coded = ''
    const gate = new KeyGate(() => void api.streamRequestKey(userId))
    const unwrap = new TsUnwrap()
    const pacer = new FramePacer<VideoFrame>()
    const meter = new RenderMeter(performance.now())
    let gateDrops = 0
    let closed = false
    let raf = 0

    const draw = (f: VideoFrame) => {
      if (canvas.width !== f.displayWidth || canvas.height !== f.displayHeight) {
        canvas.width = f.displayWidth
        canvas.height = f.displayHeight
        size = { width: f.displayWidth, height: f.displayHeight }
      }
      ctx.drawImage(f, 0, 0)
      f.close()
      waiting = false
    }
    const fresh = (c: CodecName, sz: { width: number; height: number } | null) => {
      try {
        decoder?.close()
      } catch {
        // already closed
      }
      decoder = new VideoDecoder({
        output: (f) => {
          if (closed) f.close()
          else pacer.push(f, performance.now())
        },
        error: () => {
          decoder = null // the next key frame starts a new decoder
          gate.broken()
        },
      })
      // The real picture size, read from the key frame: without it WebView2 assumes 1280×720 and
      // shows only that corner of a bigger screen.
      decoder.configure({ codec: CODEC_STRING[c], optimizeForLatency: true, ...(sz && { codedWidth: sz.width, codedHeight: sz.height }) })
      codec = c
      coded = sz ? `${sz.width}x${sz.height}` : ''
    }
    const onAu = (buf: ArrayBuffer) => {
      if (closed) return
      const au = parseAu(buf)
      if (!au) return
      const sz = au.key ? spsSize(au.codec, au.data) : null
      const resized = !!sz && `${sz.width}x${sz.height}` !== coded
      if (au.codec !== codec || !decoder || decoder.state === 'closed' || resized) {
        if (!au.key) {
          gate.broken()
          return
        }
        fresh(au.codec, sz)
      }
      meter.arrived()
      const timestamp = unwrap.next(au.ts)
      if (!gate.accept(au, decoder!.decodeQueueSize)) {
        gateDrops++
        return
      }
      decoder!.decode(new EncodedVideoChunk({ type: au.key ? 'key' : 'delta', timestamp, data: au.data }))
    }

    // Present on the display's refresh, each frame at its capture-time slot.
    const tick = (now: number) => {
      if (closed) return
      const f = pacer.take(now)
      if (f) {
        draw(f)
        meter.drawn(now)
      }
      const r = meter.report(now, { late: pacer.drops, gate: gateDrops, bufferMs: pacer.delay })
      if (r) {
        pacer.drops = 0
        gateDrops = 0
        render = r
        void api.streamRenderStats(r).catch(() => {})
      }
      raf = requestAnimationFrame(tick)
    }
    raf = requestAnimationFrame(tick)

    void (async () => {
      try {
        const codecs = await decodableCodecs()
        if (!closed) await api.streamWatch(userId, codecs, onAu)
      } catch (e) {
        store.streamNotice = errText(e)
        store.closeStream()
      }
    })()

    return () => {
      closed = true
      cancelAnimationFrame(raf)
      pacer.clear()
      void api.streamUnwatch(userId)
      try {
        decoder?.close()
      } catch {
        // already closed
      }
    }
  })

  function setVolume() {
    void api.streamSetVolume(userId, volume)
  }

  function toggleFullscreen() {
    if (document.fullscreenElement) void document.exitFullscreen()
    else void wrap.requestFullscreen()
  }
</script>

<div class="view" bind:this={wrap}>
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="screen" ondblclick={toggleFullscreen}>
    <canvas bind:this={canvas}></canvas>
    {#if waiting}<p class="wait muted">Connecting to {store.nick(userId)}'s stream…</p>{/if}
  </div>
  <div class="bar">
    <span class="who"><span class="live">LIVE</span> {store.nick(userId)}</span>
    <span class="stats muted" title={detail}>{line}</span>
    <label class="vol" title="Stream volume">
      <span class="muted">Volume</span>
      <Slider bind:value={volume} max={2} step={0.01} tone="light" label="Stream volume" oninput={setVolume} />
    </label>
    <button class="btn" onclick={toggleFullscreen}>Fullscreen</button>
    <button class="btn" onclick={() => store.closeStream()}>Stop watching</button>
  </div>
</div>

<style>
  .view { flex: 1; display: flex; flex-direction: column; min-height: 0; background: #000; border-radius: 12px; overflow: hidden; }
  .view:fullscreen { border-radius: 0; }
  .screen { flex: 1; min-height: 0; display: grid; place-items: center; position: relative; }
  /* Fills the area and letterboxes: the whole frame always shows, whatever its shape (a grid row
     sized by its content let tall frames spill out and get clipped). */
  canvas { position: absolute; inset: 0; width: 100%; height: 100%; object-fit: contain; }
  .wait { position: absolute; }
  .bar { display: flex; align-items: center; gap: 12px; padding: 8px 12px; background: var(--panel); }
  .who { font-weight: 600; display: flex; align-items: center; gap: 6px; }
  .live { background: var(--danger); color: #fff; font-size: 11px; font-weight: 700; padding: 1px 6px; border-radius: 4px; }
  .stats { flex: 1; font-size: 12px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .vol { display: flex; align-items: center; gap: 6px; font-size: 12px; }
  .vol :global(.slider) { width: 90px; }
</style>
