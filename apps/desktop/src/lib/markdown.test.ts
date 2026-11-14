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

describe('mentions in rendered messages', () => {
  const env = { users: [{ id: 1, nickname: 'Rigel' }, { id: 2, nickname: 'Budi' }], me: 1 }
  test('a mention becomes a pill, mine is marked, the text is kept as typed', () => {
    const html = render('yo @budi and @RIGEL', env)
    expect(html).toContain('<span class="mention">@budi</span>')
    expect(html).toContain('<span class="mention me">@RIGEL</span>')
  })
  test('not inside code, and names are escaped', () => {
    expect(render('`@budi`', env)).not.toContain('class="mention')
    const html = render('hi @<b>', { users: [{ id: 3, nickname: '<b>' }], me: 1 })
    expect(html).toContain('<span class="mention">@&lt;b&gt;</span>')
  })
  test('without env it renders as before', () => {
    expect(render('yo @budi')).not.toContain('mention')
  })
})
