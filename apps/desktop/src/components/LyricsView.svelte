<script lang="ts">
  import { lyricAt, wordFill, type LyricLine } from '../lib/music'

  /** `pos`: where the song is now (ms), updated every frame while playing. */
  let { lines, pos }: { lines: LyricLine[]; pos: number } = $props()

  // Lines come in a touch early, the way a singer's breath does.
  const at = $derived(pos + 150)
  const line = $derived(lyricAt(lines, at))
  const nextMs = $derived(lines[line + 1]?.ms ?? Infinity)

  // The list glides (transform only, on the compositor) so the current line sits a bit above centre.
  let list: HTMLDivElement | undefined = $state()
  let vh = $state(0)
  let offset = $state(0)
  $effect(() => {
    const i = Math.max(0, line)
    void vh
    const el = list?.querySelector<HTMLElement>(`[data-i="${i}"]`)
    if (el) offset = vh * 0.4 - (el.offsetTop + el.offsetHeight / 2)
  })
</script>

<div class="view" bind:clientHeight={vh}>
  <div class="list" bind:this={list} style="transform:translate3d(0,{offset}px,0)">
    {#each lines as l, i (i)}
      {@const d = Math.abs(i - line)}
      <!-- Lines away from the one being sung blur, more the further they are (as musicfy does). -->
      <p data-i={i} class:now={i === line} class:past={i < line} style="--d:{Math.min(d, 4)};filter:{d > 0 && d <= 9 ? `blur(${Math.min(1.5 + d * 0.9, 5).toFixed(1)}px)` : 'none'}">
        {#if i === line && l.words.length}
          {#each l.words as w, j (j)}{@const f = wordFill(l, j, at, nextMs)}{#if j}{' '}{/if}<span class="w" class:sung={f > 0} style="--p:{(f * 100).toFixed(1)}%;--i:{Math.min(j, 12)}">{w.text}</span>{/each}
        {:else}{l.text || '♪'}{/if}
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
    transform-origin: left center; transform: scale(0.94);
    opacity: calc(0.5 - var(--d) * 0.08);
    transition: transform 500ms cubic-bezier(0.22, 1, 0.36, 1), opacity 400ms ease-out, filter 400ms ease-out;
  }
  p.past { opacity: calc(0.42 - var(--d) * 0.08); }
  p.now { transform: scale(1); opacity: 1; }
  /* Word by word: each word fills with white as it is sung. */
  .w {
    display: inline-block; transition: transform 200ms ease-out; transition-delay: calc(var(--i) * 12ms);
    background: linear-gradient(90deg, #fff var(--p), rgba(255, 255, 255, 0.42) var(--p));
    -webkit-background-clip: text; background-clip: text; color: transparent;
  }
  .w.sung { transform: translateY(-3px); }
  @media (prefers-reduced-motion: reduce) {
    .list, p, .w { transition: none; }
    .w.sung { transform: none; }
  }
</style>
