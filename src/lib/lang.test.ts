import { describe, it, expect } from 'vitest';
import { langForPath } from './lang';

describe('langForPath', () => {
  it('returns javascript for ts/tsx/js/jsx/mjs/cjs', () => {
    for (const path of ['a.ts', 'a.tsx', 'a.js', 'a.jsx', 'a.mjs', 'a.cjs']) {
      expect(langForPath(path)).toBe('javascript');
    }
  });

  it('returns rust for .rs', () => {
    expect(langForPath('src/main.rs')).toBe('rust');
  });

  it('returns python for .py', () => {
    expect(langForPath('train.py')).toBe('python');
  });

  it('returns json for .json and .jsonc', () => {
    expect(langForPath('package.json')).toBe('json');
    expect(langForPath('tsconfig.jsonc')).toBe('json');
  });

  it('returns markdown for .md/.markdown/.mdx', () => {
    expect(langForPath('README.md')).toBe('markdown');
    expect(langForPath('docs/post.markdown')).toBe('markdown');
    expect(langForPath('blog.mdx')).toBe('markdown');
  });

  it('returns html for .html and .htm', () => {
    expect(langForPath('index.html')).toBe('html');
    expect(langForPath('legacy.htm')).toBe('html');
  });

  it('returns css for .css/.scss/.sass/.less', () => {
    expect(langForPath('app.css')).toBe('css');
    expect(langForPath('app.scss')).toBe('css');
    expect(langForPath('app.sass')).toBe('css');
    expect(langForPath('app.less')).toBe('css');
  });

  it('returns php for .php and .phtml', () => {
    expect(langForPath('routes/web.php')).toBe('php');
    expect(langForPath('view.phtml')).toBe('php');
  });

  it('is case-insensitive', () => {
    expect(langForPath('Foo.TS')).toBe('javascript');
    expect(langForPath('README.MD')).toBe('markdown');
  });

  it('returns null for unknown extensions', () => {
    expect(langForPath('thing.xyz')).toBeNull();
  });

  it('returns null for files with no extension', () => {
    expect(langForPath('Makefile')).toBeNull();
    expect(langForPath('LICENSE')).toBeNull();
    expect(langForPath('src/dot-only.')).toBeNull();
  });

  it('returns null when the dot lives in a parent directory only', () => {
    // `node_modules/.cache/foo` — the leading dot in a path component
    // is not a file extension.
    expect(langForPath('node_modules/.cache/foo')).toBeNull();
  });

  it('handles paths with multiple dots — only the last counts', () => {
    expect(langForPath('app.test.ts')).toBe('javascript');
    expect(langForPath('a.b.c.json')).toBe('json');
  });
});
