import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { renderMarkdown } from '../markdown';

describe('renderMarkdown — WAT-305 preview', () => {
  // --- headings ---

  it('renders ATX h1 through h6', () => {
    render(<>{renderMarkdown('# One\n\n## Two\n\n### Three\n\n#### Four\n\n##### Five\n\n###### Six')}</>);
    expect(screen.getByRole('heading', { level: 1, name: 'One' })).toBeInTheDocument();
    expect(screen.getByRole('heading', { level: 2, name: 'Two' })).toBeInTheDocument();
    expect(screen.getByRole('heading', { level: 3, name: 'Three' })).toBeInTheDocument();
    expect(screen.getByRole('heading', { level: 4, name: 'Four' })).toBeInTheDocument();
    expect(screen.getByRole('heading', { level: 5, name: 'Five' })).toBeInTheDocument();
    expect(screen.getByRole('heading', { level: 6, name: 'Six' })).toBeInTheDocument();
  });

  it('requires a space between # and heading text', () => {
    // `#tag` is a literal — common in notes that use hashtags.
    const { container } = render(<>{renderMarkdown('#tag not a heading')}</>);
    expect(container.querySelector('h1')).toBeNull();
    expect(container.textContent).toContain('#tag not a heading');
  });

  // --- lists ---

  it('renders unordered lists with both marker styles', () => {
    const { container } = render(<>{renderMarkdown('- one\n- two\n* three')}</>);
    // Consecutive `-` then `*` produce a single ul (regex treats them
    // as equivalent markers), which matches Markdown convention.
    const lists = container.querySelectorAll('ul');
    expect(lists.length).toBeGreaterThanOrEqual(1);
    const items = container.querySelectorAll('li');
    expect(items.length).toBe(3);
    expect(items[0].textContent).toBe('one');
    expect(items[2].textContent).toBe('three');
  });

  it('renders ordered lists', () => {
    const { container } = render(<>{renderMarkdown('1. first\n2. second\n3. third')}</>);
    expect(container.querySelector('ol')).not.toBeNull();
    const items = container.querySelectorAll('li');
    expect(items.length).toBe(3);
    expect(items[0].textContent).toBe('first');
    expect(items[2].textContent).toBe('third');
  });

  // --- code ---

  it('renders fenced code blocks verbatim', () => {
    const src = '```\nlet x = 1;\n// comment\n```';
    const { container } = render(<>{renderMarkdown(src)}</>);
    const pre = container.querySelector('pre');
    expect(pre).not.toBeNull();
    expect(pre?.textContent).toBe('let x = 1;\n// comment');
  });

  it('tolerates unclosed code fence (common hand-edit mistake)', () => {
    // File ends without a closing ```; renderer must not infinite-loop
    // or crash.
    const { container } = render(<>{renderMarkdown('```\nstill writing')}</>);
    const pre = container.querySelector('pre');
    expect(pre).not.toBeNull();
    expect(pre?.textContent).toBe('still writing');
  });

  it('renders inline code with backtick delimiters', () => {
    const { container } = render(<>{renderMarkdown('use `npm test` to run')}</>);
    const code = container.querySelector('code');
    expect(code).not.toBeNull();
    expect(code?.textContent).toBe('npm test');
  });

  // --- inline emphasis ---

  it('renders bold and italic', () => {
    const { container } = render(<>{renderMarkdown('**bold** and *italic*')}</>);
    expect(container.querySelector('strong')?.textContent).toBe('bold');
    expect(container.querySelector('em')?.textContent).toBe('italic');
  });

  it('does not render italic when asterisks are adjacent to spaces', () => {
    // Avoids false-positive emphasis on normal prose: "you * me * them".
    const { container } = render(<>{renderMarkdown('you * me * them')}</>);
    expect(container.querySelector('em')).toBeNull();
  });

  // --- links ---

  it('renders http and https links as clickable anchors', () => {
    render(<>{renderMarkdown('See [docs](https://example.com/docs).')}</>);
    const link = screen.getByRole('link', { name: 'docs' });
    expect(link).toHaveAttribute('href', 'https://example.com/docs');
    expect(link).toHaveAttribute('target', '_blank');
    expect(link).toHaveAttribute('rel', 'noopener noreferrer');
  });

  it('renders mailto links', () => {
    render(<>{renderMarkdown('Contact [support](mailto:help@example.com)')}</>);
    expect(screen.getByRole('link', { name: 'support' })).toHaveAttribute(
      'href',
      'mailto:help@example.com',
    );
  });

  it('refuses javascript: URLs and renders them as plain text', () => {
    // Security pin: even though we never render HTML strings, a
    // javascript: href on a React <a> would still be a vulnerability.
    // The allowlist keeps those out of the DOM entirely.
    const { container } = render(
      <>{renderMarkdown('[click](javascript:alert(1))')}</>,
    );
    expect(container.querySelector('a')).toBeNull();
    expect(container.textContent).toContain('[click](javascript:alert(1))');
  });

  it('refuses data: URLs', () => {
    const { container } = render(
      <>{renderMarkdown('[img](data:text/html;base64,PHNjcmlwdD4=)')}</>,
    );
    expect(container.querySelector('a')).toBeNull();
  });

  // --- XSS by-construction ---

  it('renders raw HTML input as literal text, not as DOM', () => {
    // We use React children (string) rather than dangerouslySetInnerHTML,
    // so an <img onerror=...> tag in the source becomes literal text.
    // Pin that property — a future refactor using innerHTML would break
    // this test.
    const nasty = '<img src=x onerror="alert(1)">';
    const { container } = render(<>{renderMarkdown(nasty)}</>);
    expect(container.querySelector('img')).toBeNull();
    expect(container.textContent).toContain(nasty);
  });

  // --- paragraphs ---

  it('splits paragraphs on blank lines', () => {
    const { container } = render(<>{renderMarkdown('first line\nsecond line\n\nthird block')}</>);
    const paras = container.querySelectorAll('p');
    expect(paras.length).toBe(2);
  });

  it('breaks out of a paragraph on a heading', () => {
    const { container } = render(<>{renderMarkdown('start of para\n# heading\ntail')}</>);
    expect(container.querySelector('h1')?.textContent).toBe('heading');
    // The paragraphs before/after are separate; "tail" is its own paragraph.
    expect(container.querySelectorAll('p').length).toBeGreaterThanOrEqual(2);
  });
});
