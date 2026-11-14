<script lang="ts">
  let { x, y, onRename, onDelete, onClose }: { x: number; y: number; onRename: () => void; onDelete: () => void; onClose: () => void } = $props()
</script>

<svelte:window onclick={onClose} onkeydown={(e) => e.key === 'Escape' && onClose()} />
<div class="menu" style="left:{x}px; top:{y}px">
  <button onclick={onRename}>Rename</button>
  <button class="danger" onclick={onDelete}>Delete channel</button>
</div>

<style>
  .menu { position: fixed; z-index: 60; background: var(--panel-2); border: 1px solid var(--line); border-radius: 8px; padding: 4px; min-width: 160px; box-shadow: 0 8px 24px rgba(0, 0, 0, 0.4); }
  button { display: block; width: 100%; text-align: left; padding: 7px 10px; border-radius: 6px; }
  button:hover { background: var(--elev); }
  .danger { color: #f87171; }
</style>
