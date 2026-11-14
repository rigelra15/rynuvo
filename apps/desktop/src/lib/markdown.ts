import MarkdownIt from 'markdown-it'

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

/** Render a chat message as safe HTML (no raw HTML, no images, no headings). */
export function render(src: string): string {
  return md.render(src)
}
