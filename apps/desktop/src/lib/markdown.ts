import MarkdownIt from 'markdown-it'
import { findMentions } from './mention'

const md = new MarkdownIt('zero', { html: false, linkify: true, breaks: true })
  .enable(['emphasis', 'strikethrough', 'backticks', 'fence', 'link', 'linkify', 'newline', 'escape', 'entity'])

// Only http(s) links; markdown-it already blocks javascript:, keep it explicit.
md.validateLink = (url: string) => /^https?:\/\//i.test(url.trim())

const defaultLinkOpen =
  md.renderer.rules.link_open ?? ((tokens, idx, options, _env, self) => self.renderToken(tokens, idx, options))

md.renderer.rules.link_open = (tokens, idx, options, env, self) => {
  tokens[idx].attrSet('target', '_blank')
  tokens[idx].attrSet('rel', 'noopener noreferrer')
  return defaultLinkOpen(tokens, idx, options, env, self)
}

/** Who can be mentioned and who "me" is; without it, mentions stay plain text. */
export interface MentionEnv { users: readonly { id: number; nickname: string }[]; me?: number }

// @mentions in plain text (not in code or links) become pills; mine are marked.
md.core.ruler.push('mention', (state) => {
  const env = state.env as Partial<MentionEnv>
  if (!env?.users) return
  for (const block of state.tokens) {
    if (block.type !== 'inline' || !block.children) continue
    const out = []
    let inLink = 0
    for (const t of block.children) {
      if (t.type === 'link_open') inLink++
      if (t.type === 'link_close') inLink--
      const found = t.type === 'text' && !inLink ? findMentions(t.content, env.users) : []
      if (!found.length) {
        out.push(t)
        continue
      }
      let at = 0
      const piece = (type: string, content: string) => {
        const p = new state.Token(type, '', 0)
        p.content = content
        return p
      }
      for (const m of found) {
        if (m.start > at) out.push(piece('text', t.content.slice(at, m.start)))
        const p = piece('mention', t.content.slice(m.start, m.end))
        p.meta = { me: m.id === 'everyone' || m.id === env.me }
        out.push(p)
        at = m.end
      }
      if (at < t.content.length) out.push(piece('text', t.content.slice(at)))
    }
    block.children = out
  }
})
md.renderer.rules.mention = (tokens, idx) =>
  `<span class="mention${tokens[idx].meta?.me ? ' me' : ''}">${md.utils.escapeHtml(tokens[idx].content)}</span>`

/** Render a chat message as safe HTML (no raw HTML, no images, no headings). */
export function render(src: string, env?: MentionEnv): string {
  return md.render(src, { ...env })
}
