<script lang="ts">
  // A button's icon and label that switch between two looks ("Mic" / "Muted"). The new one fades in
  // where the old one fades out, and the label's width glides to the new word's width, so the button
  // fits its word and never jumps.
  import Icon, { type IconName } from './Icon.svelte'

  let { icons, labels, flipped }: { icons: [IconName, IconName]; labels: [string, string]; flipped: boolean } = $props()
  let wa = $state(0)
  let wb = $state(0)
  const w = $derived(flipped ? wb : wa)
</script>

<span class="ico">
  <span class:show={!flipped}><Icon name={icons[0]} size={18} /></span>
  <span class="alt" class:show={flipped}><Icon name={icons[1]} size={18} /></span>
</span>
<span class="lbl" style={w ? `width:${w}px` : ''}>
  <span class:show={!flipped} bind:offsetWidth={wa}>{labels[0]}</span>
  <span class="alt" class:show={flipped} bind:offsetWidth={wb}>{labels[1]}</span>
</span>

<style>
  .ico { display: grid; }
  .ico > span { grid-area: 1 / 1; display: inline-flex; }
  .lbl { position: relative; display: inline-block; overflow: hidden; height: 1.45em; line-height: 1.45; transition: width 380ms cubic-bezier(0.22, 1, 0.36, 1); }
  .lbl > span { position: absolute; left: 0; top: 0; white-space: nowrap; }
  .ico > span, .lbl > span {
    opacity: 0; transform: translateY(-7px) scale(0.92); filter: blur(2px);
    transition: opacity 240ms ease, transform 340ms cubic-bezier(0.22, 1, 0.36, 1), filter 240ms ease;
  }
  .ico > span.alt:not(.show), .lbl > span.alt:not(.show) { transform: translateY(7px) scale(0.92); }
  .ico > span.show, .lbl > span.show { opacity: 1; transform: none; filter: none; }
  @media (max-width: 720px) { .lbl { display: none; } }
  @media (prefers-reduced-motion: reduce) { .lbl, .ico > span, .lbl > span { transition: none; } }
</style>
