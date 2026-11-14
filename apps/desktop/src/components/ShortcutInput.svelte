<script lang="ts">
  /** Click, then press a key combination. Produces Tauri accelerator text like "CmdOrCtrl+Shift+M". */
  let { value, onChange }: { value: string; onChange: (v: string) => void } = $props()
  let recording = $state(false)

  const isMac = navigator.userAgent.includes('Mac')
  function pretty(v: string): string {
    return v
      .replace('CmdOrCtrl', isMac ? '⌘' : 'Ctrl')
      .replace('Shift', isMac ? '⇧' : 'Shift')
      .replace('Alt', isMac ? '⌥' : 'Alt')
      .replace('Ctrl+', isMac ? '⌃' : 'Ctrl+')
      .replaceAll('+', isMac ? '' : '+')
  }

  function key(e: KeyboardEvent) {
    if (!recording) return
    e.preventDefault()
    e.stopPropagation()
    if (e.key === 'Escape') {
      recording = false
      return
    }
    if (['Shift', 'Control', 'Alt', 'Meta'].includes(e.key)) return
    const mods: string[] = []
    if (isMac ? e.metaKey : e.ctrlKey) mods.push('CmdOrCtrl')
    if (isMac && e.ctrlKey) mods.push('Ctrl')
    if (e.altKey) mods.push('Alt')
    if (e.shiftKey) mods.push('Shift')
    if (!mods.length) return // a bare key would fire while typing anywhere
    const k = e.code.startsWith('Key') ? e.code.slice(3) : e.code.startsWith('Digit') ? e.code.slice(5) : e.code
    recording = false
    onChange([...mods, k].join('+'))
  }
</script>

<button class="sc" class:rec={recording} onclick={() => (recording = true)} onkeydown={key} onblur={() => (recording = false)}>
  {recording ? 'Press keys…' : pretty(value)}
</button>

<style>
  .sc { min-width: 140px; padding: 7px 12px; border-radius: 8px; background: var(--elev); font-family: var(--mono); text-align: center; }
  .sc.rec { outline: 2px solid var(--accent); color: var(--muted); }
</style>
