<script lang="ts">
  import { activeLines, wordFill, type LyricLine, type Word } from '../lib/music'

  /** `pos`: where the song is now (ms), updated every frame while playing. */
  let { lines, pos }: { lines: LyricLine[]; pos: number } = $props()

  // Lines come in a touch early, the way a singer's breath does.
  const at = $derived(pos + 150)
  // Several lines can be sung at once (a duet, backing vocals): all of them light up.
  const active = $derived(activeLines(lines, at))
  const lo = $derived(active[0] ?? -1)
  const hi = $derived(active[active.length - 1] ?? -1)
  /** How many lines away from the ones being sung. */
  const away = (i: number) => (lo < 0 ? i + 1 : i < lo ? lo - i : i > hi ? i - hi : 0)
  const endOf = (i: number) => lines[i].end ?? lines[i + 1]?.ms ?? Infinity

  // The list glides (transform only, on the compositor) so the lines being sung sit a bit above centre.
  let list: HTMLDivElement | undefined = $state()
  let vh = $state(0)
  let offset = $state(0)
  $effect(() => {
    const first = list?.querySelector<HTMLElement>(`[data-i="${Math.max(0, lo)}"]`)
    const last = list?.querySelector<HTMLElement>(`[data-i="${Math.max(0, hi)}"]`)
    void vh
    if (first && last) offset = vh * 0.4 - (first.offsetTop + last.offsetTop + last.offsetHeight) / 2
  })
</script>

{#snippet sung(words: Word[], line: LyricLine, end: number)}
  {@const l = { ...line, words }}
  {#each words as w, j (j)}{@const f = wordFill(l, j, at, end)}{#if j}{' '}{/if}<span class="w" class:sung={f > 0} style="--p:{(f * 100).toFixed(1)}%;--i:{Math.min(j, 12)}">{w.text}</span>{/each}
{/snippet}

<div class="view" bind:clientHeight={vh}>
  <div class="list" bind:this={list} style="transform:translate3d(0,{offset}px,0)">
    {#each lines as l, i (i)}
      {@const d = away(i)}
      {@const now = d === 0}
      <!-- Lines away from the ones being sung blur a little, more the further they are. -->
      <p data-i={i} class:now class:past={i < lo} class:duet={l.duet} style="--d:{Math.min(d, 4)};filter:{d > 0 && d <= 9 ? `blur(${Math.min(0.4 + d * 0.45, 2.2).toFixed(2)}px)` : 'none'}">
        <span class="main">{#if now && l.words.length}{@render sung(l.words, l, endOf(i))}{:else}{l.text || '♪'}{/if}</span>
        {#if l.bg?.length}
          <span class="bg">{#if now}{@render sung(l.bg, l, endOf(i))}{:else}{l.bg.map((w) => w.text).join(' ')}{/if}</span>
        {/if}
      </p>
    {/each}
  </div>
</div>

<style>
  .view {
    position: relative; overflow: hidden; height: 100%;
    mask-image: linear-gradient(transparent, #000 18%, #000 72%, transparent);
  }
  .list { padding: 0 12px; will-change: transform; transition: transform 700ms cubic-bezier(0.22, 1, 0.36, 1); }
  p {
    margin: 0; padding: 7px 0; font-size: 22px; font-weight: 700; line-height: 1.3; color: #fff;
    transform-origin: left center; transform: translateZ(0) scale(0.94);
    /* Each line on its own layer: the sung line repaints every frame, and without this the blurred
       lines around it get re-rasterised with it and shimmer. */
    will-change: transform, filter, opacity; backface-visibility: hidden;
    opacity: calc(0.5 - var(--d) * 0.08);
    transition: transform 500ms cubic-bezier(0.22, 1, 0.36, 1), opacity 400ms ease-out, filter 400ms ease-out;
  }
  p.past { opacity: calc(0.42 - var(--d) * 0.08); }
  p.now { transform: translateZ(0) scale(1); opacity: 1; }
  /* The duet's second voice sings from the other side. */
  p.duet { text-align: right; transform-origin: right center; padding-left: 15%; }
  p:not(.duet) { padding-right: 4%; }
  .main, .bg { display: block; }
  /* Backing vocals: smaller, under the line, filling with it. */
  .bg { margin-top: 2px; font-size: 15px; font-weight: 600; opacity: 0.75; }
  /* Word by word: each word fills with white as it is sung. */
  .w {
    display: inline-block; transition: transform 200ms ease-out; transition-delay: calc(var(--i) * 12ms);
    background: linear-gradient(90deg, #fff var(--p), rgba(255, 255, 255, 0.42) var(--p));
    -webkit-background-clip: text; background-clip: text; color: transparent;
  }
  .w.sung { transform: translateY(-3px); }
  .bg .w.sung { transform: translateY(-2px); }
  @media (prefers-reduced-motion: reduce) {
    .list, p, .w { transition: none; }
    .w.sung { transform: none; }
  }
</style>
