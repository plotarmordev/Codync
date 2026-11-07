import assert from 'node:assert/strict'
import { test } from 'node:test'
import { build } from 'esbuild'
import { createElement, type FunctionComponent } from 'react'
import { renderToStaticMarkup } from 'react-dom/server'

// Renders the real `MarkdownText`. Node's type stripping can't load JSX, so esbuild compiles it (React stays the
// test's own) into a file under node_modules, where `react` resolves; a stub `window` carries the browser APIs its
// imports touch while loading.
async function renderMarkdown(text: string): Promise<string> {
  const outfile = new URL('../../../../node_modules/.cache/markdown-text.test.mjs', import.meta.url).pathname
  await build({
    entryPoints: [new URL('./Markdown.tsx', import.meta.url).pathname],
    outfile,
    bundle: true,
    format: 'esm',
    packages: 'external',
    jsx: 'automatic',
    loader: { '.css': 'empty' },
    logLevel: 'error',
  })
  const media = { matches: false, addEventListener() {}, removeEventListener() {} }
  Object.assign(globalThis, { window: { addEventListener() {}, matchMedia: () => media } })
  // A file built just above, so its path only exists at run time.
  const { MarkdownText }: { MarkdownText: FunctionComponent<{ text: string }> } = await import(outfile)
  return renderToStaticMarkup(createElement(MarkdownText, { text }))
}

test('MarkdownText renders every row and cell of a 150,000-row table', async () => {
  const html = await renderMarkdown(`| A | B |\n|---|---|\n${'| 1 | 2 |\n'.repeat(149_999)}| 1 | 2 | 3 |`)
  assert.match(html, /grid-template-columns:repeat\(3, max-content\)/)
  assert.equal(html.match(/class="hairline"/g)?.length, 150_000)
  assert.equal(html.match(/<span>1<\/span><span>2<\/span><span><\/span>/g)?.length, 149_999)
  assert.ok(html.includes('<span>1</span><span>2</span><span>3</span></div>'))
})
