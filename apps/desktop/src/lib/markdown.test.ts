import { describe, expect, test } from 'vitest'
import { render } from './markdown'

describe('markdown', () => {
  test('markdown escapes html', () => {
    const out = render('<script>alert(1)</script> <img src=x onerror=alert(1)>')
    expect(out).not.toContain('<script')
    expect(out).not.toContain('<img')
    expect(out).toContain('&lt;script&gt;')
  })

  test('markdown bold/code/link', () => {
    const out = render('**tebal** _miring_ `kode` ~~coret~~ https://example.com')
    expect(out).toContain('<strong>tebal</strong>')
    expect(out).toContain('<em>miring</em>')
    expect(out).toContain('<code>kode</code>')
    expect(out).toContain('<s>coret</s>')
    expect(out).toContain('<a href="https://example.com" target="_blank" rel="noopener noreferrer">')
  })

  test('no headings, images or javascript links', () => {
    const out = render('# judul\n![x](http://a/b.png)\n[klik](javascript:alert(1))')
    expect(out).not.toContain('<h1')
    expect(out).not.toContain('<img')
    expect(out).not.toMatch(/href="javascript/i)
  })

  test('fenced code block', () => {
    expect(render('```\nfn main() {}\n```')).toContain('<pre><code>fn main() {}')
  })
})
