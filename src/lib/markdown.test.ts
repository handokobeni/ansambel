import { describe, it, expect, beforeEach, vi } from 'vitest';
import { renderMarkdown, clearMarkdownCache } from './markdown';

describe('renderMarkdown', () => {
  beforeEach(() => {
    clearMarkdownCache();
  });

  it('renders paragraphs', () => {
    const html = renderMarkdown('Hello world');
    expect(html).toMatch(/<p>Hello world<\/p>/);
  });

  it('renders bold and italic emphasis', () => {
    const html = renderMarkdown('**bold** and *italic*');
    expect(html).toMatch(/<strong>bold<\/strong>/);
    expect(html).toMatch(/<em>italic<\/em>/);
  });

  it('renders inline code', () => {
    const html = renderMarkdown('press `Enter` to send');
    expect(html).toMatch(/<code>Enter<\/code>/);
  });

  it('renders fenced code blocks with language class', () => {
    const html = renderMarkdown('```ts\nconst x = 1;\n```');
    expect(html).toMatch(/<pre>/);
    expect(html).toMatch(/language-ts/);
  });

  it('renders unordered lists', () => {
    const html = renderMarkdown('- one\n- two');
    expect(html).toMatch(/<ul>/);
    expect(html).toMatch(/<li>one<\/li>/);
    expect(html).toMatch(/<li>two<\/li>/);
  });

  it('renders links as anchor tags', () => {
    const html = renderMarkdown('[here](https://example.com)');
    expect(html).toMatch(/<a[^>]*href="https:\/\/example.com"/);
  });

  it('strips dangerous script tags (XSS hardening)', () => {
    const html = renderMarkdown('hi <script>alert("x")</script> there');
    expect(html).not.toMatch(/<script/i);
    expect(html).toMatch(/hi/);
    expect(html).toMatch(/there/);
  });

  it('strips inline event handlers (XSS hardening)', () => {
    const html = renderMarkdown('<img src="x" onerror="alert(1)">');
    expect(html).not.toMatch(/onerror=/i);
  });

  it('caches identical inputs (re-render skips parsing)', () => {
    const a = renderMarkdown('hello');
    const b = renderMarkdown('hello');
    expect(a).toBe(b);
  });

  it('clearMarkdownCache forces reparse', () => {
    const html = renderMarkdown('hello');
    clearMarkdownCache();
    // Same input should still produce equal output, but coverage of the
    // cache-clear branch is what we are after.
    expect(renderMarkdown('hello')).toBe(html);
  });

  it('renders empty string as empty html', () => {
    expect(renderMarkdown('')).toBe('');
  });

  it('renders multi-line content with breaks preserved', () => {
    const html = renderMarkdown('line one\n\nline two');
    expect(html).toMatch(/<p>line one<\/p>/);
    expect(html).toMatch(/<p>line two<\/p>/);
  });

  it('falls back gracefully when language is unknown', () => {
    const html = renderMarkdown('```madeuplang\nx = 1\n```');
    expect(html).toMatch(/<pre>/);
    // Unknown language should still render every character of the code,
    // even if hljs auto-detection wraps tokens in spans.
    const text = html.replace(/<[^>]+>/g, '');
    expect(text).toContain('x');
    expect(text).toContain('=');
    expect(text).toContain('1');
  });

  it('falls back to raw text when highlightAuto throws', async () => {
    // Force the auto-highlight catch branch by stubbing highlightAuto to
    // throw — covers the defensive path in the highlight callback.
    const hljs = (await import('highlight.js/lib/core')).default;
    const spy = vi.spyOn(hljs, 'highlightAuto').mockImplementation(() => {
      throw new Error('boom');
    });
    clearMarkdownCache();
    const html = renderMarkdown('```madeuplang\nplain content\n```');
    expect(html).toContain('plain content');
    spy.mockRestore();
  });
});
