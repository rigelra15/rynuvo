import { svelte } from '@sveltejs/vite-plugin-svelte'
import { defineConfig } from 'vite'
import { execSync } from 'node:child_process'
import type { AtRule, Plugin, Rule } from 'postcss'

// On a phone or tablet, `tauri android|ios dev` sets TAURI_DEV_HOST: the dev server must listen there.
const host = process.env.TAURI_DEV_HOST

/** Hover looks only where there is a pointer to hover with. On a touch screen a tapped control keeps
 *  `:hover` until the next tap elsewhere, so every `:hover` rule moves inside `@media (hover: hover)`. */
const hoverOnly = (): Plugin => {
  const done = new WeakSet<Rule>()
  const inHoverMedia = (r: Rule) => {
    for (let p = r.parent as AtRule | undefined; p; p = p.parent as AtRule | undefined) if (p.type === 'atrule' && /\(hover:\s*hover\)/.test(p.params)) return true
    return false
  }
  return {
    postcssPlugin: 'hover-only',
    Rule(rule, { AtRule }) {
      if (done.has(rule) || !rule.selector.includes(':hover') || inHoverMedia(rule)) return
      const hovers = rule.selectors.filter((s) => s.includes(':hover'))
      const rest = rule.selectors.filter((s) => !s.includes(':hover'))
      const moved = rule.clone({ selectors: hovers })
      done.add(moved)
      const media = new AtRule({ name: 'media', params: '(hover: hover)' })
      media.append(moved)
      rule.after(media)
      if (rest.length) rule.selectors = rest
      else rule.remove()
    },
  }
}

/** The build number shown on the splash: commits so far and the commit itself ("145 · 0122a63"). */
const build = (() => {
  try {
    const git = (a: string) => execSync(`git ${a}`, { stdio: ['ignore', 'pipe', 'ignore'] }).toString().trim()
    return `${git('rev-list --count HEAD')} · ${git('rev-parse --short HEAD')}`
  } catch {
    return 'local'
  }
})()

// https://vite.dev/config/
export default defineConfig({
  plugins: [svelte()],
  define: { __BUILD__: JSON.stringify(build) },
  css: { postcss: { plugins: [hoverOnly()] } },
  server: {
    host: host || false,
    hmr: host ? { protocol: 'ws', host, port: 1421 } : undefined,
  },
})
