<script lang="ts">
  import { isMobile } from '../lib/platform'
  import Slider from './Slider.svelte'
  import Icon from './Icon.svelte'
  import CaptionsOverlay from './CaptionsOverlay.svelte'
  import { onMount } from 'svelte'
  import { api, errText } from '../lib/api'
  import { CODEC_STRING, decodableCodecs, KeyGate, parseAu, spsSize, statsLine, type CodecName } from '../lib/stream'
  import { store } from '../lib/state.svelte'
  import type { Id } from '../lib/types'

  let { userId }: { userId: Id } = $props()

  let canvas: HTMLCanvasElement
  let wrap: HTMLDivElement
  let size = $state({ width: 0, height: 0 })
  let volume = $state(1)
  let waiting = $state(true)
  const stats = $derived(store.streamStats.get(userId))
  const line = $derived(
    statsLine({ width: size.width, height: size.height, fps: stats?.fps ?? 0, codec: stats?.codec ?? '', path: stats?.path ?? 'connecting' }),
  )

  onMount(() => {
    const ctx = canvas.getContext('2d')!
    let decoder: VideoDecoder | null = null
    let codec: CodecName | null = null
    let coded = ''
    const gate = new KeyGate(() => void api.streamRequestKey(userId))
    let n = 0
    let closed = false

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
        output: draw,
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
      if (!gate.accept(au, decoder!.decodeQueueSize)) return
      decoder!.decode(new EncodedVideoChunk({ type: au.key ? 'key' : 'delta', timestamp: n++ * 16_667, data: au.data }))
    }

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

  // Phones and tablets: the WebView has no real fullscreen, so the stream covers the app instead,
  // and the phone may turn sideways while it does. Back (button or gesture) leaves it.
  let covering = $state(false)
  const rotate = (free: boolean) => (window as { RynuvoAndroid?: { setFreeRotation(f: boolean): void } }).RynuvoAndroid?.setFreeRotation(free)
  // Ambient mode: the picture's own colours, blurred, glow around it and fill the black bars, like a
  // lamp behind a TV. A tiny copy is refreshed a few times a second and eased towards, so it breathes
  // with the picture instead of flickering. Remembered on this device.
  const AMBIENT_KEY = 'rynuvo.ambient'
  let ambient = $state((() => {
    try {
      return localStorage.getItem(AMBIENT_KEY) !== '0'
    } catch {
      return true
    }
  })())
  let glow: HTMLCanvasElement
  let fill: HTMLCanvasElement
  function toggleAmbient() {
    ambient = !ambient
    try {
      localStorage.setItem(AMBIENT_KEY, ambient ? '1' : '0')
    } catch {
      // private mode: just not remembered
    }
  }
  $effect(() => {
    if (!ambient) return
    const gs = [glow, fill].map((c) => c.getContext('2d')!)
    const t = setInterval(() => {
      if (waiting || !canvas.width) return
      for (const g of gs) {
        g.globalAlpha = 0.35
        g.drawImage(canvas, 0, 0, g.canvas.width, g.canvas.height)
      }
    }, 120)
    return () => clearInterval(t)
  })

  // Zoom into the picture (pinch, ctrl/⌘ + wheel or a trackpad pinch, double-tap on a phone) and
  // drag it around; only the picture scales, never the controls around it.
  let screen: HTMLDivElement
  let zoom = $state({ z: 1, x: 0, y: 0 })
  const MAX_ZOOM = 5
  function setZoom(z: number, x: number, y: number) {
    z = Math.min(MAX_ZOOM, Math.max(1, z))
    // Keep the picture covering the screen: no panning past its edges.
    const w = screen?.clientWidth ?? 0
    const h = screen?.clientHeight ?? 0
    const mx = ((z - 1) * w) / 2
    const my = ((z - 1) * h) / 2
    zoom = z === 1 ? { z: 1, x: 0, y: 0 } : { z, x: Math.min(mx, Math.max(-mx, x)), y: Math.min(my, Math.max(-my, y)) }
  }
  /** Zoom to `z` keeping the point (px, py), in screen pixels from its centre, still. */
  function zoomAt(z: number, px: number, py: number) {
    z = Math.min(MAX_ZOOM, Math.max(1, z))
    const k = z / zoom.z
    setZoom(z, px - (px - zoom.x) * k, py - (py - zoom.y) * k)
  }
  const fromCentre = (e: { clientX: number; clientY: number }) => {
    const r = screen.getBoundingClientRect()
    return [e.clientX - r.left - r.width / 2, e.clientY - r.top - r.height / 2] as const
  }
  const pointers = new Map<number, { x: number; y: number }>()
  let pinch: { d: number; cx: number; cy: number } | null = null
  let dragged = false
  function down(e: PointerEvent) {
    if (e.pointerType === 'mouse' && e.button !== 0) return
    pointers.set(e.pointerId, { x: e.clientX, y: e.clientY })
    screen.setPointerCapture(e.pointerId)
    if (pointers.size === 1) dragged = false
    if (pointers.size === 2) pinch = null
  }
  function move(e: PointerEvent) {
    const p = pointers.get(e.pointerId)
    if (!p) return
    if (pointers.size >= 2) {
      p.x = e.clientX
      p.y = e.clientY
      const [a, b] = [...pointers.values()]
      const d = Math.hypot(a.x - b.x, a.y - b.y)
      const [cx, cy] = fromCentre({ clientX: (a.x + b.x) / 2, clientY: (a.y + b.y) / 2 })
      if (pinch && pinch.d > 0) {
        const k = d / pinch.d
        const z = Math.min(MAX_ZOOM, Math.max(1, zoom.z * k))
        const r = z / zoom.z
        // Scale about the fingers' midpoint, and follow it as it moves.
        setZoom(z, cx - (pinch.cx - zoom.x) * r, cy - (pinch.cy - zoom.y) * r)
      }
      pinch = { d, cx, cy }
      dragged = true
      return
    }
    const dx = e.clientX - p.x
    const dy = e.clientY - p.y
    if (Math.abs(dx) + Math.abs(dy) > 3) dragged = true
    p.x = e.clientX
    p.y = e.clientY
    if (zoom.z > 1) setZoom(zoom.z, zoom.x + dx, zoom.y + dy)
  }
  function up(e: PointerEvent) {
    pointers.delete(e.pointerId)
    pinch = null
  }
  function wheel(e: WheelEvent) {
    // A trackpad pinch arrives as ctrl + wheel.
    if (e.ctrlKey || e.metaKey) {
      e.preventDefault()
      const [px, py] = fromCentre(e)
      zoomAt(zoom.z * Math.exp(-e.deltaY * 0.01), px, py)
    } else if (zoom.z > 1) {
      e.preventDefault()
      setZoom(zoom.z, zoom.x - e.deltaX, zoom.y - e.deltaY)
    }
  }
  function doubleTap(e: MouseEvent) {
    // Phones: double-tap zooms in there (or back out). Desktop: double-click is full screen.
    if (!isMobile) return toggleFullscreen()
    const [px, py] = fromCentre(e)
    if (zoom.z > 1) setZoom(1, 0, 0)
    else zoomAt(2.5, px, py)
  }
  onMount(() => {
    screen.addEventListener('wheel', wheel, { passive: false })
    return () => screen.removeEventListener('wheel', wheel)
  })

  // Full screen on a phone: the controls float over the picture and fade after a few seconds; a tap
  // on the picture brings them back (or hides them again).
  let idle = $state(false)
  let idleTimer: ReturnType<typeof setTimeout> | undefined
  function wake() {
    idle = false
    clearTimeout(idleTimer)
    if (covering) idleTimer = setTimeout(() => (idle = true), 3000)
  }
  function tapScreen() {
    if (dragged) return // that was a pan or a pinch
    if (!covering) return
    if (idle) wake()
    else {
      clearTimeout(idleTimer)
      idle = true
    }
  }

  // Covering the app means leaving it: the room around the stream makes its own stacking context
  // (its header and seats would show through), so the view moves to <body> while it covers.
  function portal(node: HTMLElement, on: boolean) {
    const home = document.createComment('')
    node.before(home)
    const set = (v: boolean) => {
      if (v) document.body.appendChild(node)
      else if (home.parentNode && node.previousSibling !== home) home.after(node)
    }
    set(on)
    // Gone while covering: it lives in <body> then, so take it out from there too.
    return { update: set, destroy: () => (home.remove(), node.remove()) }
  }

  $effect(() => {
    if (!covering) return
    wake()
    rotate(true)
    history.pushState({ ...history.state, fullStream: true }, '')
    const pop = () => { if (!history.state?.fullStream) covering = false }
    window.addEventListener('popstate', pop)
    return () => {
      window.removeEventListener('popstate', pop)
      clearTimeout(idleTimer)
      idle = false
      rotate(false)
      if (history.state?.fullStream) history.back()
    }
  })

  function toggleFullscreen() {
    if (isMobile) covering = !covering
    else if (document.fullscreenElement) void document.exitFullscreen()
    else void wrap.requestFullscreen()
  }
</script>

<div class="view" class:covering class:idle bind:this={wrap} use:portal={covering} style="--ar:{size.width && size.height ? `${size.width} / ${size.height}` : '16 / 9'}">
  <canvas class="glow" class:on={ambient && !waiting} bind:this={glow} width="48" height="27" aria-hidden="true"></canvas>
  <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
  <div class="screen" bind:this={screen} ondblclick={doubleTap} onclick={tapScreen} onpointerdown={down} onpointermove={move} onpointerup={up} onpointercancel={up} class:zoomed={zoom.z > 1}>
    <canvas class="fill" class:on={ambient && !waiting} bind:this={fill} width="48" height="27" aria-hidden="true"></canvas>
    <canvas class="pic" bind:this={canvas} style="transform: translate({zoom.x}px, {zoom.y}px) scale({zoom.z})"></canvas>
    {#if covering}<CaptionsOverlay over />{/if}
    {#if zoom.z > 1}
      <button class="zoomout" onclick={(e) => (e.stopPropagation(), setZoom(1, 0, 0))} onpointerdown={(e) => e.stopPropagation()} aria-label="Reset zoom">
        <Icon name="shrink" size={14} />{Math.round(zoom.z * 100)}%
      </button>
    {/if}
    {#if waiting}<p class="wait muted">Connecting to {store.nick(userId)}'s stream…</p>{/if}
  </div>
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="bar" onpointerdown={() => covering && wake()}>
    <span class="who"><span class="live">LIVE</span><span class="nm">{store.nick(userId)}</span></span>
    <span class="stats muted">{line}</span>
    {#if store.myVoice.deafened}
      <!-- Deafened silences everything, the stream's sound too: say so where it's missed. -->
      <button class="deaf" onclick={() => store.toggleDeafen()}>
        <Icon name="headphonesOff" size={15} /><span>You're deafened · <b>Turn sound on</b></span>
      </button>
    {:else}
    <label class="vol" title="Stream volume">
      <span class="vlabel muted">Volume</span>
      <span class="vicon" aria-hidden="true"><Icon name="volume" size={16} /></span>
      <Slider bind:value={volume} max={1} step={0.01} tone="light" label="Stream volume" oninput={setVolume} />
    </label>
    {/if}
    <span class="acts">
      <button class="btn amb" class:active={ambient} onclick={toggleAmbient} aria-pressed={ambient} aria-label="Ambient mode" title="Ambient mode">
        <span class="ic"><Icon name="ambient" size={16} /></span><span class="lbl">Ambient</span>
      </button>
      <button class="btn" onclick={toggleFullscreen} aria-label={covering ? 'Exit fullscreen' : 'Fullscreen'} title={covering ? 'Exit fullscreen' : 'Fullscreen'}>
        <span class="ic"><Icon name={covering ? 'shrink' : 'expand'} size={16} /></span><span class="lbl">{covering ? 'Exit fullscreen' : 'Fullscreen'}</span>
      </button>
      <button class="btn" onclick={() => store.closeStream()} aria-label="Stop watching" title="Stop watching">
        <span class="ic"><Icon name="close" size={16} /></span><span class="lbl">Stop watching</span>
      </button>
    </span>
  </div>
</div>

<style>
  .view { flex: 1; display: flex; flex-direction: column; min-height: 0; border-radius: 12px; position: relative; isolation: isolate; }
  .view > .screen { background: #000; border-radius: 12px 12px 0 0; }
  .view > .bar { border-radius: 0 0 12px 12px; }
  .view.covering > .screen, .view.covering > .bar, .view:fullscreen > .screen, .view:fullscreen > .bar { border-radius: 0; }
  .view.covering, .view:fullscreen { background: #000; }
  /* Ambient: a glow spilling out around the picture, and its colours filling the black bars. */
  .glow, .fill { pointer-events: none; opacity: 0; transition: opacity 600ms ease; }
  .glow { position: absolute; left: -8%; top: -22%; width: 116%; height: 144%; z-index: -1; filter: blur(52px) saturate(1.6); border-radius: 60px; }
  .glow.on { opacity: 0.7; }
  .fill { position: absolute; inset: -10%; width: 120%; height: 120%; filter: blur(34px) saturate(1.4) brightness(0.7); }
  .fill.on { opacity: 0.9; }
  .covering .glow { display: none; }
  .amb.active { color: var(--accent); }
  .view.covering .amb.active { color: var(--accent); }
  /* A phone: as tall as the picture needs, centred where the room was. */
  @media (max-width: 760px) {
    .view:not(.covering) { flex: 0 1 auto; margin-block: auto; }
    .view:not(.covering) .screen { flex: none; aspect-ratio: var(--ar); max-height: 62vh; }
  }
  .view:fullscreen { border-radius: 0; }
  /* Full screen on a phone: the picture gets the whole screen; the controls float over its foot. */
  .view.covering .bar {
    position: absolute; left: 0; right: 0; bottom: 0; z-index: 1;
    padding: 36px max(18px, env(safe-area-inset-right)) calc(env(safe-area-inset-bottom) + 14px) max(18px, env(safe-area-inset-left));
    background: linear-gradient(to top, rgba(8, 6, 12, 0.92), rgba(8, 6, 12, 0.7) 50%, transparent);
    transition: opacity 240ms ease, transform 240ms ease;
  }
  .view.covering.idle .bar { opacity: 0; transform: translateY(14px); pointer-events: none; }
  .view.covering .who, .view.covering .stats { color: #fff; text-shadow: 0 1px 3px rgba(0, 0, 0, 0.8); }
  .view.covering .stats { opacity: 0.7; }
  .view.covering .lbl, .view.covering .vlabel { display: none; }
  .view.covering .ic, .view.covering .vicon { display: inline-flex; color: rgba(255, 255, 255, 0.85); }
  .view.covering .acts .btn {
    width: 42px; height: 42px; padding: 0; justify-content: center; border-radius: 14px; color: #fff;
    background: rgba(255, 255, 255, 0.14); border-color: rgba(255, 255, 255, 0.12); backdrop-filter: blur(12px); -webkit-backdrop-filter: blur(12px);
  }
  .view.covering .vol :global(.slider) { width: 160px; }
  .view.covering {
    position: fixed; inset: 0; z-index: 200; border-radius: 0;
    padding: env(safe-area-inset-top) env(safe-area-inset-right) env(safe-area-inset-bottom) env(safe-area-inset-left);
  }
  .screen { flex: 1; min-height: 0; display: grid; place-items: center; position: relative; overflow: hidden; touch-action: none; }
  .screen.zoomed { cursor: grab; }
  .screen.zoomed:active { cursor: grabbing; }
  .pic { transform-origin: center; will-change: transform; }
  .zoomout {
    position: absolute; top: 12px; right: 12px; z-index: 2; display: inline-flex; align-items: center; gap: 6px;
    padding: 6px 12px; border-radius: 999px; font-size: 12.5px; font-weight: 700; color: #fff;
    background: rgba(8, 6, 12, 0.6); border: 1px solid rgba(255, 255, 255, 0.16); backdrop-filter: blur(10px); -webkit-backdrop-filter: blur(10px);
  }
  .view.covering .zoomout { top: calc(env(safe-area-inset-top) + 14px); right: max(14px, env(safe-area-inset-right)); }
  /* Fills the area and letterboxes: the whole frame always shows, whatever its shape (a grid row
     sized by its content let tall frames spill out and get clipped). */
  .pic { position: absolute; inset: 0; width: 100%; height: 100%; object-fit: contain; }
  .wait { position: absolute; }
  .bar { display: flex; align-items: center; gap: 12px; padding: 8px 12px; background: var(--panel); }
  .who { font-weight: 600; display: flex; align-items: center; gap: 6px; min-width: 0; }
  .nm { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .acts { display: flex; gap: 8px; flex: none; }
  .deaf {
    display: inline-flex; align-items: center; gap: 8px; height: 34px; padding: 0 14px; border-radius: 999px; font-size: 13px; white-space: nowrap;
    color: #fecaca; background: rgba(239, 68, 68, 0.16); border: 1px solid rgba(239, 68, 68, 0.35);
  }
  .deaf b { color: #fff; font-weight: 700; }
  .btn { display: inline-flex; align-items: center; gap: 6px; }
  .ic, .vicon { display: none; }
  .view { container-type: inline-size; }

  /* Narrow (a phone, a small window): the picture keeps its own shape, centred in the room instead of
     a tall black box; the bar folds into two short rows with icon buttons. */
  @container (max-width: 560px) {
    .bar { flex-wrap: wrap; gap: 8px 10px; padding: 10px 12px 12px; }
    .who { flex: 1; font-size: 15px; }
    .stats, .vlabel, .lbl { display: none; }
    .ic, .vicon { display: inline-flex; }
    .acts .btn { width: 38px; height: 38px; padding: 0; justify-content: center; border-radius: 12px; }
    .vol { order: 3; flex: 1 0 100%; gap: 10px; color: var(--muted); }
    .deaf { order: 3; flex: 1 0 100%; justify-content: center; height: 40px; }
    .vol :global(.slider) { width: auto; flex: 1; }
  }
  .live { background: var(--danger); color: #fff; font-size: 11px; font-weight: 700; padding: 1px 6px; border-radius: 4px; }
  .stats { flex: 1; font-size: 12px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .vol { display: flex; align-items: center; gap: 6px; font-size: 12px; }
  .vol :global(.slider) { width: 90px; }
</style>
