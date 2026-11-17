<script lang="ts">
  import { cubicIn, cubicOut } from 'svelte/easing'
  import { fade, fly, type TransitionConfig } from 'svelte/transition'
  import type { UpdatePhase } from '../lib/update'
  import { updater } from '../lib/updater.svelte'
  import { isAndroid } from '../lib/platform'
  import { api } from '../lib/api'

  let version = $state('')
  void api.appVersion().then((v) => (version = v)).catch(() => {})

  /** Set while a new version is on its way: the splash becomes the update screen. */
  let { update = null, waiting = null }: {
    update?: Exclude<UpdatePhase, { phase: 'idle' }> | null
    /** Still not connected after a moment: no internet on the device, or the server can't be reached. */
    waiting?: 'offline' | 'server' | null
  } = $props()

  const progress = $derived(update?.phase === 'downloading' ? (update.progress ?? null) : update && update.phase !== 'failed' ? 1 : null)
  /** No size known yet, or installing: the outline runs round instead of filling up. */
  const spinning = $derived(update !== null && (update.phase === 'installing' || (update.phase === 'downloading' && update.progress == null)))
  const pct = $derived(progress == null ? null : Math.round(progress * 100))
  /** Each step of the update gets its own line; they cross-fade into one another. */
  const step = $derived(update ? (update.phase === 'install' && update.needsPermission ? 'permission' : update.phase) : (waiting ?? 'loading'))

  // Covers the app while it loads. The lamp comes on over the desk: four facets fly in and lock
  // into the diamond, light sweeps across it, and it starts to sound, sending out rings like a
  // voice. The name writes itself in under it. When the app is ready it all opens up into it.
  const word = 'Rynuvo'.split('')

  /** Leaving: the diamond swells a touch and the whole cover dissolves into the app. */
  function open(node: Element): TransitionConfig {
    const reduced = matchMedia('(prefers-reduced-motion: reduce)').matches
    return {
      duration: reduced ? 150 : 640,
      easing: cubicIn,
      css: (t, u) => (reduced ? `opacity:${t}` : `opacity:${t};transform:scale(${1 + u * 0.08});filter:blur(${(u * 10).toFixed(2)}px)`),
    }
  }
</script>

<div class="splash" class:updating={update !== null} in:fade={{ duration: 260 }} out:open role="status" aria-label={update ? `Updating Rynuvo to ${update.version}` : 'Loading Rynuvo'}>
  <div class="bloom warm"></div>
  <div class="bloom lilac"></div>
  <div class="grain"></div>

  <div class="mark">
    <!-- Rings sent out by the diamond: the shape of the logo, travelling outward like sound. -->
    {#each [0, 1, 2] as i (i)}<span class="ring" style="--i:{i}"></span>{/each}
    <span class="halo"></span>
    <svg class="gem" viewBox="-60 -60 120 120" aria-hidden="true">
      <defs>
        <linearGradient id="sp-top" x1="0" y1="-1" x2="0" y2="0" gradientUnits="objectBoundingBox">
          <stop offset="0" stop-color="#ffe2a8" /><stop offset="1" stop-color="#f9c877" />
        </linearGradient>
        <linearGradient id="sp-shine" x1="0" y1="0" x2="1" y2="0">
          <stop offset="0" stop-color="#fff" stop-opacity="0" />
          <stop offset="0.5" stop-color="#fff" stop-opacity="0.75" />
          <stop offset="1" stop-color="#fff" stop-opacity="0" />
        </linearGradient>
        <clipPath id="sp-clip"><path d="M0 -50 L50 0 L0 50 L-50 0 Z" /></clipPath>
      </defs>
      <!-- Update progress: an outline that draws itself round the diamond, from the top. -->
      {#if update}
        <g class="outline" class:spinning in:fade={{ duration: 400 }}>
          <path class="track" d="M0 -60 L60 0 L0 60 L-60 0 Z" pathLength="100" />
          <path class="fill" d="M0 -60 L60 0 L0 60 L-60 0 Z" pathLength="100" style:stroke-dashoffset={spinning ? null : 100 - (progress ?? 0) * 100} />
        </g>
      {/if}
      <!-- The diamond in four facets (cut along its diagonals), each lit a little differently. -->
      <path class="facet f1" d="M0 -50 L50 0 L0 0 Z" fill="#ffd690" />
      <path class="facet f2" d="M50 0 L0 50 L0 0 Z" fill="#eda24a" />
      <path class="facet f3" d="M0 50 L-50 0 L0 0 Z" fill="#f4b860" />
      <path class="facet f4" d="M-50 0 L0 -50 L0 0 Z" fill="#ffe0a3" />
      <g clip-path="url(#sp-clip)">
        <rect class="shine" x="-30" y="-60" width="34" height="120" fill="url(#sp-shine)" />
      </g>
    </svg>
  </div>

  <div class="word" aria-hidden="true">
    {#each word as ch, i (i)}<span style="--i:{i}">{ch}</span>{/each}
  </div>

  <div class="status">
    {#key step}
      <div class="line" in:fly={{ y: 10, duration: 420, delay: 140, easing: cubicOut }} out:fade={{ duration: 140 }}>
        {#if !update && waiting}
          <div class="wait">
            <span class="dot"></span>
            {waiting === 'offline' ? 'No internet connection' : "Can't reach Rynuvo"}
          </div>
          <div class="note">{waiting === 'offline' ? "Rynuvo will open as soon as you're back online" : 'Trying again…'}</div>
        {:else if !update}
          <!-- Loading: a little equaliser, the room warming up. -->
          <div class="eq" aria-hidden="true">
            {#each [0, 1, 2, 3, 4] as i (i)}<span style="--i:{i}"></span>{/each}
          </div>
        {:else if update.phase === 'downloading'}
          <div class="label">Updating to {update.version}</div>
          <div class="big" aria-live="polite">{pct == null ? 'Downloading…' : `${pct}%`}</div>
        {:else if update.phase === 'ready'}
          <div class="label">Rynuvo {update.version} is ready</div>
          <div class="big" aria-live="polite">Restarting in {update.seconds}s</div>
        {:else if update.phase === 'installing'}
          <div class="label">Installing {update.version}</div>
          <div class="note">Rynuvo will open again in a moment</div>
        {:else if update.phase === 'install' && update.needsPermission}
          <div class="label">One more step</div>
          <div class="note">Allow Rynuvo to install updates, then come back here</div>
          <div class="actions">
            <button class="go" onclick={() => void updater.install()}>Open settings</button>
            <button class="ghost" onclick={() => updater.postpone()}>Later</button>
          </div>
        {:else if update.phase === 'install'}
          <div class="label">Rynuvo {update.version} is ready</div>
          <div class="actions">
            <button class="go" onclick={() => void updater.install()}>Install update</button>
            <button class="ghost" onclick={() => updater.postpone()}>Later</button>
          </div>
        {:else if update.phase === 'failed'}
          <div class="label">The update couldn't finish</div>
          <div class="note err">{update.error}</div>
          <div class="actions">
            <button class="go" onclick={() => void updater.restart()}>Try again</button>
            {#if isAndroid}<button class="ghost" onclick={() => updater.postpone()}>Later</button>{/if}
          </div>
        {/if}
      </div>
    {/key}
  </div>

  <!-- Version and build, quietly at the foot of the screen. -->
  <div class="build" aria-hidden="true">{version ? `Version ${version} · ` : ''}Build {__BUILD__}</div>
</div>

<style>
  .splash {
    position: fixed; inset: 0; z-index: 1000; overflow: hidden; background: var(--desk);
    display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 22px;
  }

  /* The lamp's warm light and the room's lilac, drifting slowly over the desk. */
  .bloom { position: absolute; width: 90vmax; height: 90vmax; border-radius: 50%; pointer-events: none; opacity: 0; animation: bloom-in 1.94s ease-out forwards; }
  .warm {
    left: -40vmax; top: -45vmax; background: radial-gradient(closest-side, rgba(244, 184, 96, 0.28), rgba(244, 184, 96, 0.06) 60%, transparent);
    animation: bloom-in 1.94s ease-out forwards, drift-a 12.32s ease-in-out infinite alternate;
  }
  .lilac {
    right: -42vmax; bottom: -48vmax; background: radial-gradient(closest-side, rgba(139, 92, 246, 0.42), rgba(109, 69, 201, 0.12) 55%, transparent);
    animation: bloom-in 2.11s 0.22s ease-out forwards, drift-b 14.08s ease-in-out infinite alternate;
  }
  /* A whisper of film grain so the gradients don't band. */
  .grain {
    position: absolute; inset: 0; pointer-events: none; opacity: 0.05; mix-blend-mode: overlay;
    background-image: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='120' height='120'%3E%3Cfilter id='n'%3E%3CfeTurbulence type='fractalNoise' baseFrequency='0.9' numOctaves='2'/%3E%3C/filter%3E%3Crect width='120' height='120' filter='url(%23n)'/%3E%3C/svg%3E");
  }

  .mark { position: relative; width: 120px; height: 120px; display: grid; place-items: center; }
  .gem { position: relative; z-index: 2; width: 104px; height: 104px; overflow: visible; filter: drop-shadow(0 10px 30px rgba(244, 184, 96, 0.35)); animation: settle 0.66s 1.23s cubic-bezier(0.34, 1.56, 0.64, 1) both; }
  .facet { transform-box: view-box; transform-origin: 0 0; animation: fly 0.97s cubic-bezier(0.22, 1, 0.36, 1) both; }
  .f1 { --x: 34px; --y: -34px; --r: 40deg; animation-delay: 0.09s; }
  .f2 { --x: 34px; --y: 34px; --r: -40deg; animation-delay: 0.24s; }
  .f3 { --x: -34px; --y: 34px; --r: 40deg; animation-delay: 0.39s; }
  .f4 { --x: -34px; --y: -34px; --r: -40deg; animation-delay: 0.54s; }
  .shine { animation: shine 2.99s 1.67s ease-in-out infinite; transform: translateX(-70px); }

  /* The glow under the diamond, and the rings it sends out once it's whole. */
  .halo {
    position: absolute; inset: -40px; border-radius: 50%; z-index: 1;
    background: radial-gradient(closest-side, rgba(244, 184, 96, 0.38), transparent); opacity: 0;
    animation: halo-in 1.06s 1.1s ease-out forwards, breathe 2.82s 2.16s ease-in-out infinite;
  }
  .ring {
    position: absolute; width: 74px; height: 74px; border-radius: 12px; z-index: 0;
    border: 1.5px solid rgba(244, 184, 96, 0.7); transform: rotate(45deg) scale(1); opacity: 0;
    animation: ring 2.82s calc(1.63s + var(--i) * 0.92s) cubic-bezier(0.2, 0.6, 0.3, 1) infinite;
  }

  .word { display: flex; font-size: 34px; font-weight: 750; font-stretch: 78%; letter-spacing: -0.01em; color: var(--text); font-variation-settings: 'opsz' 48; }
  .word span { display: inline-block; opacity: 0; animation: letter 0.79s calc(1.32s + var(--i) * 0.08s) cubic-bezier(0.22, 1, 0.36, 1) forwards; }

  /* Below the name: loading, or the update's progress and choices. One cell, so steps overlap as
     they cross-fade instead of pushing each other about. */
  .status { display: grid; min-height: 96px; justify-items: center; align-items: start; text-align: center; padding: 0 24px; }
  .line { grid-area: 1 / 1; display: flex; flex-direction: column; align-items: center; gap: 6px; }
  .label { font-size: 12px; font-weight: 700; letter-spacing: 0.12em; text-transform: uppercase; color: var(--accent); }
  .big { font-size: 26px; font-weight: 750; font-variant-numeric: tabular-nums; color: var(--text); font-stretch: 85%; }
  .note { font-size: 14px; color: var(--muted, rgba(255, 255, 255, 0.6)); max-width: 300px; line-height: 1.45; }
  .wait { display: flex; align-items: center; gap: 8px; font-size: 15px; font-weight: 650; color: var(--text); }
  .wait .dot { width: 8px; height: 8px; border-radius: 50%; background: #f4b860; animation: blink 1.6s ease-in-out infinite; }
  @keyframes blink { 50% { opacity: 0.25; transform: scale(0.8); } }
  .err { color: #fca5a5; overflow-wrap: anywhere; font-size: 13px; }
  .actions { display: flex; gap: 10px; margin-top: 12px; }
  .actions button { border: 0; font: inherit; font-weight: 700; font-size: 14px; padding: 11px 20px; border-radius: 12px; cursor: pointer; transition: transform 160ms, background 160ms; }
  .actions button:active { transform: scale(0.96); }
  .go { background: linear-gradient(135deg, #ffd690, #f4b860 55%, #e59a3c); color: #1a1407; box-shadow: 0 8px 24px rgba(244, 184, 96, 0.3); }
  .ghost { background: rgba(255, 255, 255, 0.07); color: var(--text); }

  /* The progress outline round the diamond. */
  .outline path { fill: none; stroke-width: 3; stroke-linejoin: round; stroke-linecap: round; }
  .track { stroke: rgba(244, 184, 96, 0.16); }
  .fill { stroke: #f4b860; stroke-dasharray: 100; stroke-dashoffset: 100; transition: stroke-dashoffset 380ms cubic-bezier(0.22, 1, 0.36, 1); filter: drop-shadow(0 0 6px rgba(244, 184, 96, 0.6)); }
  .spinning .fill { stroke-dasharray: 22 78; transition: none; animation: run 1.3s linear infinite; }
  /* While updating, the outline takes over from the sound rings. */
  .updating .ring { opacity: 0 !important; animation-play-state: paused; transition: opacity 300ms; }

  .build {
    position: absolute; left: 0; right: 0; bottom: calc(22px + env(safe-area-inset-bottom)); text-align: center;
    font-size: 11.5px; letter-spacing: 0.04em; font-variant-numeric: tabular-nums; color: var(--text); opacity: 0;
    animation: faint-in 1s 1.2s ease-out forwards;
  }
  @keyframes faint-in { to { opacity: 0.32; } }

  .eq { display: flex; align-items: flex-end; gap: 4px; height: 16px; opacity: 0; animation: fade-in 0.7s 2.11s ease-out forwards; }
  .eq span {
    width: 3px; height: 100%; border-radius: 2px; background: linear-gradient(to top, var(--accent), #c4b5ff);
    transform-origin: bottom; animation: eq 0.97s calc(var(--i) * -0.2s) ease-in-out infinite alternate;
  }

  @keyframes fly {
    from { opacity: 0; transform: translate(var(--x), var(--y)) rotate(var(--r)) scale(0.6); }
    to { opacity: 1; transform: none; }
  }
  @keyframes settle { from { transform: scale(1); } 50% { transform: scale(1.08); } to { transform: scale(1); } }
  @keyframes shine { 0% { transform: translateX(-70px); } 45%, 100% { transform: translateX(80px); } }
  @keyframes halo-in { to { opacity: 1; } }
  @keyframes breathe { 0%, 100% { transform: scale(0.92); } 50% { transform: scale(1.08); } }
  @keyframes ring {
    0% { opacity: 0.85; transform: rotate(45deg) scale(1); }
    100% { opacity: 0; transform: rotate(45deg) scale(2.6); }
  }
  @keyframes letter {
    from { opacity: 0; transform: translateY(14px); filter: blur(8px); }
    to { opacity: 1; transform: none; filter: blur(0); }
  }
  @keyframes eq { from { transform: scaleY(0.2); } to { transform: scaleY(1); } }
  @keyframes fade-in { to { opacity: 1; } }
  @keyframes run { to { stroke-dashoffset: -100; } }
  @keyframes bloom-in { to { opacity: 1; } }
  @keyframes drift-a { to { transform: translate(8vmax, 6vmax) scale(1.08); } }
  @keyframes drift-b { to { transform: translate(-7vmax, -5vmax) scale(1.1); } }

  /* Reduced motion: everything simply there, softly lit. */
  @media (prefers-reduced-motion: reduce) {
    .bloom, .halo, .word span, .eq { animation: none; opacity: 1; }
    .facet, .gem, .shine, .eq span { animation: none; }
    .ring { display: none; }
    .build { animation: none; opacity: 0.32; }
  }
</style>
