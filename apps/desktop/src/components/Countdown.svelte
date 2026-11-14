<script lang="ts">
  let { until }: { until: number } = $props()
  let now = $state(Date.now())
  $effect(() => {
    const t = setInterval(() => (now = Date.now()), 1000)
    return () => clearInterval(t)
  })
  const left = $derived(Math.max(0, Math.floor((until - now) / 1000)))
</script>

{#if left > 0}valid for {Math.floor(left / 60)}:{String(left % 60).padStart(2, '0')}{:else}expired{/if}
