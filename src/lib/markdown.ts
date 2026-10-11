import MarkdownIt from 'markdown-it';
import taskLists from 'markdown-it-task-lists';
import katex from 'katex';
import hljs from 'highlight.js/lib/core';
import javascript from 'highlight.js/lib/languages/javascript';
import typescript from 'highlight.js/lib/languages/typescript';
import python from 'highlight.js/lib/languages/python';
import rust from 'highlight.js/lib/languages/rust';
import json from 'highlight.js/lib/languages/json';
import css from 'highlight.js/lib/languages/css';
import xml from 'highlight.js/lib/languages/xml';
import DOMPurify from 'dompurify';
import 'katex/dist/katex.min.css';
import 'highlight.js/styles/github-dark.css';

for (const [name, language] of Object.entries({ javascript, js: javascript, typescript, ts: typescript, python, py: python, rust, rs: rust, json, css, html: xml, xml })) hljs.registerLanguage(name, language);

const mathBlock = (state: any, startLine: number, endLine: number, silent: boolean) => {
  const line = state.src.slice(state.bMarks[startLine] + state.tShift[startLine], state.eMarks[startLine]);
  const open = line.startsWith('$$') ? '$$' : line.startsWith('\\[') ? '\\[' : '';
  if (!open) return false;
  const close = open === '$$' ? '$$' : '\\]';
  let next = startLine, content = line.slice(open.length);
  if (content.endsWith(close)) content = content.slice(0, -close.length);
  else {
    let found = false;
    for (next = startLine + 1; next < endLine; next++) {
      const part = state.src.slice(state.bMarks[next] + state.tShift[next], state.eMarks[next]);
      if (part.trim() === close) { found = true; break; }
      content += `\n${part}`;
    }
    if (!found) return false;
  }
  if (silent) return true;
  const token = state.push('math_block', '', 0); token.block = true; token.content = content.trim(); token.map = [startLine, next + 1];
  state.line = next + 1; return true;
};

const md = new MarkdownIt({ html: false, linkify: false, breaks: false,
  highlight: (code, lang) => {
    const name = lang.toLowerCase();
    if (!name || !hljs.getLanguage(name)) return '';
    try { return `<pre class="hljs"><code>${hljs.highlight(code, { language: name }).value}</code></pre>`; } catch { return ''; }
  }
});
md.use(taskLists, { enabled: true, label: true, labelAfter: true });
md.validateLink = (url: string) => {
  if (/^(https?:|mailto:|#)/i.test(url)) return true;
  return !/^[a-z][a-z\d+.-]*:/i.test(url) && !url.startsWith('//');
};
md.block.ruler.before('fence', 'math_block', mathBlock, { alt: ['paragraph', 'reference'] });
md.inline.ruler.before('escape', 'math_inline', (state: any, silent: boolean) => {
  const src = state.src.slice(state.pos);
  const patterns = [/^\$([^\n$]+)\$/u, /^\\\((.+?)\\\)/u, /^\\\[(.+?)\\\]/u];
  const match = patterns.map((p, i) => ({ m: src.match(p), display: i === 2 })).find(x => x.m);
  if (!match?.m) return false;
  if (!silent) { const t = state.push('math_inline', '', 0); t.content = match.m[1]; t.meta = { display: match.display }; }
  state.pos += match.m[0].length; return true;
});
md.renderer.rules.math_inline = tokens => {
  try { return katex.renderToString(tokens[0].content, { displayMode: !!tokens[0].meta?.display, throwOnError: false, trust: false }); }
  catch { return `<code>${md.utils.escapeHtml(tokens[0].content)}</code>`; }
};
md.renderer.rules.math_block = tokens => {
  try { return `${katex.renderToString(tokens[0].content, { displayMode: true, throwOnError: false, trust: false })}\n`; }
  catch { return `<pre class="math-error">${md.utils.escapeHtml(tokens[0].content)}</pre>\n`; }
};

export function renderMarkdown(source: string): string {
  try {
    const purifier = DOMPurify as unknown as { sanitize?: (html: string, config: object) => string };
    const instance = typeof purifier.sanitize === 'function' ? purifier : (DOMPurify as unknown as (window: Window) => typeof purifier)(window);
    return instance.sanitize!(md.render(source), { USE_PROFILES: { html: true }, ADD_ATTR: ['class', 'aria-hidden', 'type', 'checked'] });
  }
  catch (error) { console.error('Markdown preview error', error); return '<p class="render-error">Preview could not be rendered.</p>'; }
}
