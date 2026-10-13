import { Marked, type Tokens } from 'marked';
import DOMPurify from 'dompurify';
import hljs from 'highlight.js/lib/common';
import { openUrl } from '@tauri-apps/plugin-opener';

export function escapeHtml(text: string): string {
  return text
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');
}

const EXT_LANG: Record<string, string> = {
  ts: 'typescript',
  tsx: 'typescript',
  mts: 'typescript',
  cts: 'typescript',
  js: 'javascript',
  jsx: 'javascript',
  mjs: 'javascript',
  cjs: 'javascript',
  vue: 'xml',
  html: 'xml',
  htm: 'xml',
  svg: 'xml',
  xml: 'xml',
  rs: 'rust',
  py: 'python',
  rb: 'ruby',
  go: 'go',
  java: 'java',
  kt: 'kotlin',
  cs: 'csharp',
  cpp: 'cpp',
  cc: 'cpp',
  hpp: 'cpp',
  c: 'c',
  h: 'c',
  css: 'css',
  scss: 'scss',
  less: 'less',
  json: 'json',
  md: 'markdown',
  yml: 'yaml',
  yaml: 'yaml',
  toml: 'ini',
  ini: 'ini',
  sh: 'bash',
  bash: 'bash',
  ps1: 'powershell',
  sql: 'sql',
  php: 'php',
  swift: 'swift',
  lua: 'lua',
};

export function languageForPath(path: string): string | undefined {
  const ext = path.split('.').pop()?.toLowerCase() ?? '';
  return EXT_LANG[ext];
}

/** Highlight code to HTML; unknown languages are escaped as plain text. */
export function highlightCode(code: string, lang?: string): string {
  const language = lang && hljs.getLanguage(lang) ? lang : undefined;
  if (!language) return escapeHtml(code);
  try {
    return hljs.highlight(code, { language, ignoreIllegals: true }).value;
  } catch {
    return escapeHtml(code);
  }
}

const COPY_ICON =
  '<svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="9" y="9" width="13" height="13" rx="2"/><path d="M5 15H4a2 2 0 01-2-2V4a2 2 0 012-2h9a2 2 0 012 2v1"/></svg>';

const marked = new Marked({
  gfm: true,
  breaks: false,
  renderer: {
    code({ text, lang }: Tokens.Code): string {
      const language = (lang ?? '').trim().split(/\s+/)[0].toLowerCase();
      const label = language || 'text';
      return `<div class="code-block"><div class="code-block-header"><span>${escapeHtml(label)}</span><button type="button" class="code-copy" data-copy>${COPY_ICON} Copy</button></div><pre><code class="hljs">${highlightCode(text, language)}</code></pre></div>`;
    },
  },
});

/** Render untrusted model Markdown to sanitized HTML. */
export function renderMarkdown(source: string): string {
  const html = marked.parse(source, { async: false }) as string;
  return DOMPurify.sanitize(html, { ADD_ATTR: ['data-copy'] });
}

export async function copyText(text: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    return false;
  }
}

/**
 * Click handler for rendered markdown: copies code blocks and opens links in
 * the system browser instead of navigating the webview.
 */
export function handleMarkdownClick(event: MouseEvent): void {
  const target = event.target as HTMLElement | null;
  if (!target) return;

  const copyBtn = target.closest<HTMLButtonElement>('[data-copy]');
  if (copyBtn) {
    event.preventDefault();
    const code = copyBtn.closest('.code-block')?.querySelector('code')?.textContent ?? '';
    void copyText(code).then((ok) => {
      if (!ok) return;
      const original = copyBtn.innerHTML;
      copyBtn.textContent = 'Copied';
      window.setTimeout(() => (copyBtn.innerHTML = original), 1400);
    });
    return;
  }

  const link = target.closest<HTMLAnchorElement>('a[href]');
  if (link) {
    event.preventDefault();
    const href = link.getAttribute('href') ?? '';
    if (/^(https?:|mailto:)/i.test(href)) void openUrl(href);
  }
}
