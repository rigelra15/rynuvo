<script lang="ts">
  import Icon from './Icon.svelte'
  import UserPanel from './UserPanel.svelte'
  import type { User } from '../lib/types'

  let { me, onSettings }: { me: User; onSettings: () => void } = $props()
</script>

<aside class="side">
  <header class="brand">Home</header>
  <nav class="lists">
    <button class="ch on"><span class="ico"><Icon name="users" size={18} /></span>Friends</button>
    <div class="grp">Direct messages</div>
    <p class="soon">Private chats with one friend are coming soon.</p>
  </nav>
  <UserPanel {me} {onSettings} />
</aside>

<style>
  .side { background: var(--panel); display: flex; flex-direction: column; min-height: 0; }
  .brand { font-weight: 700; padding: 16px 16px 10px; color: #fff; }
  .lists { flex: 1; overflow-y: auto; padding: 0 8px 8px; }
  .ch { display: flex; align-items: center; gap: 10px; width: 100%; text-align: left; padding: 8px 10px; border-radius: 6px; color: var(--muted); font-weight: 500; }
  .ch.on { background: var(--elev); color: #fff; }
  .ico { display: inline-flex; }
  .grp { margin: 18px 8px 4px; font-size: 11px; letter-spacing: 0.06em; text-transform: uppercase; color: var(--muted); font-weight: 600; }
  .soon { margin: 4px 8px; font-size: 12.5px; color: var(--faint); line-height: 1.4; }
</style>
