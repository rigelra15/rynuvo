<script lang="ts">
  // The app's one slider (volumes, thresholds): thin track filled up to the value, small round knob.
  // `tone`: "light" (white) on album art and video, "accent" on the app's own panels.
  let {
    value = $bindable(),
    min = 0,
    max = 1,
    step = 0.01,
    label,
    tone = 'accent',
    oninput,
  }: {
    value: number
    min?: number
    max?: number
    step?: number
    label: string
    tone?: 'accent' | 'light'
    oninput?: (v: number) => void
  } = $props()

  const pct = $derived(max > min ? ((Math.min(max, Math.max(min, value)) - min) / (max - min)) * 100 : 0)
</script>

<input
  class="slider {tone}"
  type="range"
  {min}
  {max}
  {step}
  aria-label={label}
  bind:value
  style="--v:{pct}%"
  oninput={(e) => oninput?.(Number(e.currentTarget.value))}
/>

<style>
  .slider {
    --on: var(--accent);
    --off: var(--elev);
    --knob: #fff;
    width: 100%; height: 16px; margin: 0; background: transparent; cursor: pointer;
    -webkit-appearance: none; appearance: none;
  }
  .slider.light { --on: #fff; --off: rgba(255, 255, 255, 0.22); }
  .slider::-webkit-slider-runnable-track {
    height: 4px; border-radius: 2px;
    background: linear-gradient(90deg, var(--on) var(--v), var(--off) var(--v));
  }
  .slider::-webkit-slider-thumb {
    -webkit-appearance: none; appearance: none; width: 12px; height: 12px; margin-top: -4px; border-radius: 50%; background: var(--knob);
    box-shadow: 0 1px 4px rgba(0, 0, 0, 0.4); transform: scale(0.85); transition: transform 120ms;
  }
  .slider:hover::-webkit-slider-thumb, .slider:active::-webkit-slider-thumb { transform: scale(1.05); }
  .slider:focus-visible { outline: none; }
  .slider:focus-visible::-webkit-slider-thumb { box-shadow: 0 0 0 3px color-mix(in srgb, var(--on) 40%, transparent); }
  @media (prefers-reduced-motion: reduce) { .slider::-webkit-slider-thumb { transition: none; } }
</style>
