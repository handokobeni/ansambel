import { Marked } from 'marked';
import { markedHighlight } from 'marked-highlight';
import hljs from 'highlight.js/lib/core';
import DOMPurify from 'dompurify';

// Register a curated language set so the bundle stays small while covering
// the languages users most commonly drop into chat. Aliases mirror the
// ones supported by highlight.js's automatic detection.
import javascript from 'highlight.js/lib/languages/javascript';
import typescript from 'highlight.js/lib/languages/typescript';
import python from 'highlight.js/lib/languages/python';
import rust from 'highlight.js/lib/languages/rust';
import bash from 'highlight.js/lib/languages/bash';
import json from 'highlight.js/lib/languages/json';
import css from 'highlight.js/lib/languages/css';
import xml from 'highlight.js/lib/languages/xml';
import yaml from 'highlight.js/lib/languages/yaml';
import markdown from 'highlight.js/lib/languages/markdown';
import sql from 'highlight.js/lib/languages/sql';
import go from 'highlight.js/lib/languages/go';
import diff from 'highlight.js/lib/languages/diff';
import ini from 'highlight.js/lib/languages/ini';

hljs.registerLanguage('javascript', javascript);
hljs.registerLanguage('js', javascript);
hljs.registerLanguage('jsx', javascript);
hljs.registerLanguage('typescript', typescript);
hljs.registerLanguage('ts', typescript);
hljs.registerLanguage('tsx', typescript);
hljs.registerLanguage('python', python);
hljs.registerLanguage('py', python);
hljs.registerLanguage('rust', rust);
hljs.registerLanguage('bash', bash);
hljs.registerLanguage('sh', bash);
hljs.registerLanguage('shell', bash);
hljs.registerLanguage('zsh', bash);
hljs.registerLanguage('json', json);
hljs.registerLanguage('css', css);
hljs.registerLanguage('html', xml);
hljs.registerLanguage('xml', xml);
hljs.registerLanguage('svg', xml);
hljs.registerLanguage('svelte', xml);
hljs.registerLanguage('yaml', yaml);
hljs.registerLanguage('yml', yaml);
hljs.registerLanguage('markdown', markdown);
hljs.registerLanguage('md', markdown);
hljs.registerLanguage('sql', sql);
hljs.registerLanguage('go', go);
hljs.registerLanguage('diff', diff);
hljs.registerLanguage('toml', ini);
hljs.registerLanguage('ini', ini);

const marked = new Marked(
  markedHighlight({
    langPrefix: 'hljs language-',
    highlight(code: string, lang: string): string {
      if (lang && hljs.getLanguage(lang)) {
        return hljs.highlight(code, { language: lang }).value;
      }
      // Fall back to auto-detection for unknown / unspecified languages.
      try {
        return hljs.highlightAuto(code).value;
      } catch {
        // Auto-detect can throw for malformed input; degrade to raw text.
        return code;
      }
    },
  })
);

marked.setOptions({
  gfm: true,
  breaks: false,
});

// Cache rendered HTML keyed by raw markdown. Chat messages are immutable
// once persisted, so the cache never goes stale during a session.
const renderCache = new Map<string, string>();

/**
 * Render markdown to sanitized HTML. Synchronous and safe to call inline
 * from Svelte templates. Output is run through DOMPurify so it cannot
 * carry script tags or inline event handlers.
 */
export function renderMarkdown(raw: string): string {
  if (raw === '') return '';
  const cached = renderCache.get(raw);
  if (cached !== undefined) return cached;
  const html = marked.parse(raw) as string;
  const sanitized = DOMPurify.sanitize(html);
  renderCache.set(raw, sanitized);
  return sanitized;
}

/** Drop all cached markdown renders. Test hook only. */
export function clearMarkdownCache(): void {
  renderCache.clear();
}
