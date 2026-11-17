<script lang="ts">
  import { encode } from 'uqr'

  // The transfer QR as Rynuvo's diamond, made of dust. Each module flies in from somewhere around
  // and settles into place; once settled a slow shimmer runs across them and loose dust keeps
  // circling the card. Underneath it is a plain QR turned 45° (scanners read any angle) with error
  // correction at its highest so the lamp can sit in the middle. Modules are softened squares that
  // touch: round dots with gaps read noticeably worse at a distance (checked with ZXing).
  let { text, label = 'QR code' }: { text: string; label?: string } = $props()

  const qr = $derived(encode(text, { ecc: 'H', border: 0 }))
  const n = $derived(qr.size)
  /** Quiet zone around the code, in modules. */
  const pad = 3
  const side = $derived(n + pad * 2)
  /** The rotated card's bounding box. */
  const box = $derived(Math.ceil(side * Math.SQRT2) + 2)
  /** Modules left clear for the lamp in the middle (odd, so it sits on the centre module). */
  const hole = $derived(Math.round(n * 0.26) | 1)

  const inFinder = (r: number, c: number) => (r < 7 && c < 7) || (r < 7 && c >= n - 7) || (r >= n - 7 && c < 7)
  const inHole = (r: number, c: number) => {
    const m = (n - 1) / 2
    return Math.abs(r - m) + Math.abs(c - m) <= hole / 2 + 0.5
  }

  // A small seeded random, so the same code always gathers the same way.
  function rng(seed: string) {
    let h = 2166136261
    for (const ch of seed) h = Math.imul(h ^ ch.charCodeAt(0), 16777619)
    return () => ((h = Math.imul(h ^ (h >>> 15), 2246822507) ^ Math.imul(h ^ (h >>> 13), 3266489909)) >>> 0) / 4294967296
  }

  interface Dot { x: number; y: number; dx: number; dy: number; delay: number; wave: number }
  const dots = $derived.by(() => {
    const rand = rng(text)
    const out: Dot[] = []
    const m = n / 2
    qr.data.forEach((row, r) =>
      row.forEach((on, c) => {
        if (!on || inFinder(r, c) || inHole(r, c)) return
        // From a random point out in the cloud.
        const a = rand() * Math.PI * 2
        const dist = n * (0.55 + rand() * 0.9)
        out.push({
          x: c,
          y: r,
          dx: Math.cos(a) * dist,
          dy: Math.sin(a) * dist,
          // Centre first, outward after, with some scatter: it condenses rather than fills in.
          delay: Math.hypot(c - m, r - m) * 26 + rand() * 520,
          // The shimmer runs diagonally across the diamond.
          wave: (c + r) * 70,
        })
      }),
    )
    return out
  })
  const finders = $derived([
    [0, 0],
    [n - 7, 0],
    [0, n - 7],
  ])

  // Loose dust circling the card (outside it, so it never sits on the code).
  const dust = $derived.by(() => {
    const rand = rng(text + ':dust')
    return Array.from({ length: 46 }, (_, i) => ({
      r: 50 + rand() * 22,
      // Where on the circle it starts, so the dust is spread out even when nothing moves.
      angle: rand() * Math.PI * 2,
      size: 0.6 + rand() * 1.4,
      dur: 14 + rand() * 22,
      start: rand() * -40,
      twinkle: 1.6 + rand() * 2.6,
      color: i % 3 === 0 ? '#c4b5ff' : '#f4b860',
      reverse: rand() < 0.3,
    }))
  })
</script>

{#key text}
  <div class="dqr" role="img" aria-label={label}>
    <svg class="dust" viewBox="-100 -100 200 200" aria-hidden="true">
      {#each dust as d}
        <g class="orbit" class:reverse={d.reverse} style="--dur:{d.dur}s;--start:{d.start}s">
          <circle cx={(Math.cos(d.angle) * d.r).toFixed(1)} cy={(Math.sin(d.angle) * d.r).toFixed(1)} r={d.size} fill={d.color} style="--tw:{d.twinkle}s" />
        </g>
      {/each}
    </svg>

    <svg class="code" viewBox="{-box / 2} {-box / 2} {box} {box}">
      <defs>
        <linearGradient id="dqr-lamp" x1="0" y1="0" x2="1" y2="1">
          <stop offset="0" stop-color="#ffd690" />
          <stop offset="1" stop-color="#d9973f" />
        </linearGradient>
        <!-- The ink: plum into wine into burnt amber, across the code. Every stop stays dark enough
             against the cream to read like black. -->
        <linearGradient id="dqr-ink" gradientUnits="userSpaceOnUse" x1="0" y1="0" x2={n} y2={n}>
          <stop offset="0" stop-color="#2b1a46" />
          <stop offset="0.5" stop-color="#3a1230" />
          <stop offset="1" stop-color="#52260c" />
        </linearGradient>
        <radialGradient id="dqr-card" cx="0.5" cy="0.5" r="0.7">
          <stop offset="0" stop-color="#fffaf1" />
          <stop offset="1" stop-color="#fbe8c8" />
        </radialGradient>
      </defs>
      <g transform="rotate(45)">
        <rect class="card" x={-side / 2} y={-side / 2} width={side} height={side} rx="3.2" />
        <g transform="translate({-n / 2} {-n / 2})" fill="url(#dqr-ink)">
          {#each dots as d}
            <rect class="dot" x={d.x} y={d.y} width="1" height="1" rx="0.3"
              style="--dx:{d.dx.toFixed(1)}px;--dy:{d.dy.toFixed(1)}px;--d:{d.delay.toFixed(0)}ms;--w:{d.wave}ms" />
          {/each}
          {#each finders as [x, y], i}
            <g class="eye" style="--d:{900 + i * 140}ms">
              <rect class="eye-o" x={x + 0.5} y={y + 0.5} width="6" height="6" rx="1.9" />
              <rect class="eye-i" x={x + 2} y={y + 2} width="3" height="3" rx="0.9" />
            </g>
          {/each}
        </g>
      </g>
      <!-- The lamp, upright in the middle, lit last. -->
      <path class="lamp" d="M0 {-hole * 0.55} L{hole * 0.55} 0 L0 {hole * 0.55} L{-hole * 0.55} 0 Z" fill="url(#dqr-lamp)" />
    </svg>
  </div>
{/key}

<style>
  .dqr { position: relative; width: 100%; height: 100%; }
  .code { position: relative; display: block; width: 100%; height: 100%; overflow: visible; filter: drop-shadow(0 8px 22px rgba(244, 184, 96, 0.28)); }
  .dust { position: absolute; inset: -26%; width: 152%; height: 152%; pointer-events: none; overflow: visible; }

  .card { fill: url(#dqr-card); stroke: #f4b860; stroke-width: 0.5; animation: card 700ms cubic-bezier(0.22, 1, 0.36, 1) both; transform-box: fill-box; transform-origin: center; }
  @keyframes card { from { opacity: 0; transform: scale(0.86); } }

  /* Each module: flies in as a mote of gold dust, lands and darkens into the ink, then a glint of
     gold runs along its edge now and then. The glint is only an outline: the module stays dark. */
  .dot {
    stroke: #f4b860; stroke-width: 0; transform-box: fill-box; transform-origin: center; paint-order: stroke;
    animation: gather 1100ms var(--d) cubic-bezier(0.16, 1, 0.3, 1) both, glint 3.4s calc(2.6s + var(--w)) ease-in-out infinite;
  }
  @keyframes gather {
    0% { transform: translate(var(--dx), var(--dy)) scale(0.25); opacity: 0; stroke-width: 2.4; }
    35% { opacity: 1; }
    65% { stroke-width: 1.4; }
    100% { transform: none; opacity: 1; stroke-width: 0; }
  }
  @keyframes glint { 0%, 70%, 100% { stroke-width: 0; } 82% { stroke-width: 0.16; } }

  .eye { transform-box: fill-box; transform-origin: center; animation: eye 700ms var(--d) cubic-bezier(0.34, 1.56, 0.64, 1) both; }
  @keyframes eye { from { opacity: 0; transform: scale(0.4); } }
  .eye-o { fill: none; stroke: #2b1a46; stroke-width: 1; }
  .eye-i { fill: #7a400d; }
  .eye:nth-of-type(2) .eye-o { stroke: #3d1a40; }
  .eye:nth-of-type(3) .eye-o { stroke: #3d1a40; }

  .lamp { transform-box: fill-box; transform-origin: center; animation: lamp 800ms 1500ms cubic-bezier(0.34, 1.56, 0.64, 1) both, glow 3s 2.4s ease-in-out infinite; }
  @keyframes lamp { from { opacity: 0; transform: scale(0) rotate(-90deg); } }
  @keyframes glow { 50% { filter: drop-shadow(0 0 1.6px #ffd690); } }

  .orbit { transform-box: view-box; transform-origin: center; animation: spin var(--dur) var(--start) linear infinite; }
  .orbit.reverse { animation-direction: reverse; }
  .orbit circle { animation: twinkle var(--tw) ease-in-out infinite alternate, dust-in 1.4s ease-out both; }
  @keyframes spin { to { transform: rotate(360deg); } }
  @keyframes twinkle { from { opacity: 0.15; } to { opacity: 0.85; } }
  @keyframes dust-in { from { opacity: 0; } }

  @media (prefers-reduced-motion: reduce) {
    .dot, .eye, .lamp, .card, .orbit, .orbit circle { animation: none; }
  }
</style>
