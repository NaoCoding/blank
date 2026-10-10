import { describe, expect, it } from 'vitest';
import { renderMarkdown } from '../src/lib/markdown';

describe('Markdown preview rendering', () => {
  it('renders GFM tables, code, and inline and display math', () => {
    const html = renderMarkdown('| a | b |\n|---|---|\n|1|2|\n\n- [ ] todo\n\n```js\nconst x = 1\n```\n\n$x^2$ and \\(y^2\\)\n\n$$\nx^2\n$$\n\n\\[\ny = mx + b\n\\]');
    expect(html).toContain('<table>');
    expect(html).toContain('hljs');
    expect(html).toContain('katex');
    expect(html).toContain('task-list-item');
    expect(html).toContain('katex-display');
  });
  it('does not run raw HTML or dangerous links and tolerates malformed math', () => {
    const html = renderMarkdown('<script>alert(1)</script>\n\n[bad](javascript:alert(1))\n\n\\(\\frac{\\)');
    expect(html).not.toContain('<script>');
    expect(html).not.toContain('href="javascript:');
    expect(html).toContain('katex');
  });
});
