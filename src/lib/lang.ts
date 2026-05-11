// Pure helper that maps a file path's extension to a CodeMirror
// language identifier. The Editor component swaps this string for the
// actual CodeMirror `LanguageSupport` extension at mount time so the
// heavy `@codemirror/lang-*` packages can be lazy-imported.

export type LangId =
  | 'javascript'
  | 'rust'
  | 'python'
  | 'json'
  | 'markdown'
  | 'html'
  | 'css'
  | 'php'
  | null;

const EXT_MAP: Record<string, LangId> = {
  ts: 'javascript',
  tsx: 'javascript',
  js: 'javascript',
  jsx: 'javascript',
  mjs: 'javascript',
  cjs: 'javascript',
  rs: 'rust',
  py: 'python',
  json: 'json',
  jsonc: 'json',
  md: 'markdown',
  markdown: 'markdown',
  mdx: 'markdown',
  html: 'html',
  htm: 'html',
  css: 'css',
  scss: 'css',
  sass: 'css',
  less: 'css',
  php: 'php',
  phtml: 'php',
};

/** Returns the language id for a file's extension, or null when no
 *  language matches. Path-based so callers don't have to extract the
 *  extension themselves. */
export function langForPath(path: string): LangId {
  const idx = path.lastIndexOf('.');
  if (idx < 0 || idx === path.length - 1) return null;
  // Strip leading directory components when the dot lives in a parent.
  const lastSlash = path.lastIndexOf('/');
  if (idx < lastSlash) return null;
  const ext = path.slice(idx + 1).toLowerCase();
  return EXT_MAP[ext] ?? null;
}
