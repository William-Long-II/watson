import React from 'react';

/**
 * WAT-305: minimal markdown renderer for the note preview toggle.
 *
 * Implemented as a pure-React tree: we never build HTML strings or use
 * `dangerouslySetInnerHTML`, so the rendered output carries no XSS
 * surface regardless of what the user types. The tradeoff is coverage —
 * we support only the subset listed below. Users who write HTML tags in
 * their notes see the tags as literal text in preview.
 *
 * Supported:
 * - ATX headings (# .. ######)
 * - Unordered lists (- or *)
 * - Ordered lists (1. 2. 3.)
 * - Code fences (``` ... ```)
 * - Inline code (`...`)
 * - Bold (**...**)
 * - Italic (*...*) — only well-formed single-asterisk pairs; list
 *   markers are consumed at block level so no ambiguity here
 * - Links [text](url) — URL passes an allowlist (http, https, mailto,
 *   relative, anchor); anything else (javascript:, data:, etc.) is
 *   rendered as plain text
 * - Paragraphs (blank-line separated)
 *
 * Not supported (literal text):
 * - Tables, blockquotes, horizontal rules, images, reference links,
 *   footnotes, task lists, HTML passthrough.
 */

/** Whether a href is safe to render as a clickable link. */
function isSafeUrl(url: string): boolean {
  const lower = url.trim().toLowerCase();
  return (
    lower.startsWith('http://') ||
    lower.startsWith('https://') ||
    lower.startsWith('mailto:') ||
    lower.startsWith('/') ||
    lower.startsWith('#')
  );
}

/**
 * Render a single line of inline markdown into a list of React nodes.
 * Single-pass: at each position, try the token patterns in priority
 * order; if none match, append the character to the plain-text buffer.
 */
function renderInline(text: string, keyPrefix: string): React.ReactNode[] {
  const nodes: React.ReactNode[] = [];
  let buffer = '';
  let i = 0;
  let nodeCounter = 0;

  const flush = () => {
    if (buffer) {
      nodes.push(buffer);
      buffer = '';
    }
  };
  const pushNode = (node: React.ReactNode) => {
    flush();
    nodes.push(node);
    nodeCounter++;
  };

  while (i < text.length) {
    const ch = text[i];

    // Inline code: `...`. Highest priority — contents are literal, no
    // nested markup.
    if (ch === '`') {
      const end = text.indexOf('`', i + 1);
      if (end !== -1) {
        pushNode(
          <code key={`${keyPrefix}-c${nodeCounter}`} className="px-1 py-0.5 bg-[var(--input-bg)] rounded text-xs font-mono">
            {text.slice(i + 1, end)}
          </code>,
        );
        i = end + 1;
        continue;
      }
    }

    // Bold: **...**. Must look for closing ** that's not immediately
    // after the opening (empty emphasis like **** is ignored).
    if (ch === '*' && text[i + 1] === '*') {
      const end = text.indexOf('**', i + 2);
      if (end !== -1 && end > i + 2) {
        pushNode(
          <strong key={`${keyPrefix}-b${nodeCounter}`}>
            {renderInline(text.slice(i + 2, end), `${keyPrefix}-b${nodeCounter}`)}
          </strong>,
        );
        i = end + 2;
        continue;
      }
    }

    // Italic: *...*. Single asterisk, not doubled. Require non-space
    // immediately after the opener and before the closer so `a * b * c`
    // doesn't become "a <em> b </em> c".
    if (ch === '*' && text[i + 1] !== '*' && text[i + 1] !== ' ' && text[i + 1] !== undefined) {
      // Scan for a closing single asterisk with non-space before it.
      let j = i + 1;
      while (j < text.length) {
        if (text[j] === '*' && text[j + 1] !== '*' && text[j - 1] !== ' ') {
          pushNode(
            <em key={`${keyPrefix}-i${nodeCounter}`}>
              {renderInline(text.slice(i + 1, j), `${keyPrefix}-i${nodeCounter}`)}
            </em>,
          );
          i = j + 1;
          break;
        }
        j++;
      }
      if (j < text.length) continue;
      // No valid closer — fall through to plain-text handling.
    }

    // Link: [text](url). Greedy on text (no nested brackets), URL can't
    // contain unescaped ')'.
    if (ch === '[') {
      const match = /^\[([^\]]+)\]\(([^)]+)\)/.exec(text.slice(i));
      if (match) {
        const [full, linkText, url] = match;
        if (isSafeUrl(url)) {
          pushNode(
            <a
              key={`${keyPrefix}-a${nodeCounter}`}
              href={url}
              target="_blank"
              rel="noopener noreferrer"
              className="text-blue-500 hover:underline"
            >
              {renderInline(linkText, `${keyPrefix}-a${nodeCounter}`)}
            </a>,
          );
        } else {
          // Unsafe protocol — render as plain text, keeping the syntax
          // visible so the user knows the link didn't render. Safer than
          // silently stripping.
          pushNode(
            <span key={`${keyPrefix}-u${nodeCounter}`} className="text-gray-400" title="Unsafe URL not rendered as a link">
              {full}
            </span>,
          );
        }
        i += full.length;
        continue;
      }
    }

    buffer += ch;
    i++;
  }
  flush();
  return nodes;
}

/**
 * Parse block-level markdown and render to a React tree. Blocks are
 * separated by blank lines; each non-blank region becomes a single
 * block element. Code fences and lists don't require preceding blank
 * lines — they break out of the current paragraph on sight.
 */
export function renderMarkdown(source: string): React.ReactNode {
  const lines = source.split(/\r?\n/);
  const blocks: React.ReactNode[] = [];
  let i = 0;

  const isHeadingLine = (line: string) => /^#{1,6}\s+\S/.test(line);
  const isUlLine = (line: string) => /^[-*]\s+\S/.test(line);
  const isOlLine = (line: string) => /^\d+\.\s+\S/.test(line);
  const isBreakout = (line: string) =>
    line.startsWith('```') ||
    isHeadingLine(line) ||
    isUlLine(line) ||
    isOlLine(line);

  while (i < lines.length) {
    const line = lines[i];

    // Code fence: opening ``` to closing ```, content rendered verbatim.
    if (line.startsWith('```')) {
      const start = i + 1;
      let end = start;
      while (end < lines.length && !lines[end].startsWith('```')) {
        end++;
      }
      blocks.push(
        <pre key={`pre-${i}`} className="p-2 my-2 bg-[var(--input-bg)] rounded overflow-x-auto text-xs font-mono">
          <code>{lines.slice(start, end).join('\n')}</code>
        </pre>,
      );
      // Skip past the closing fence if present; tolerate missing closer.
      i = end < lines.length ? end + 1 : end;
      continue;
    }

    // Heading.
    const hMatch = /^(#{1,6})\s+(.+)$/.exec(line);
    if (hMatch) {
      const level = hMatch[1].length;
      const content = renderInline(hMatch[2], `h-${i}`);
      const key = `h-${i}`;
      const className =
        level === 1
          ? 'text-xl font-bold mt-3 mb-2'
          : level === 2
            ? 'text-lg font-semibold mt-3 mb-2'
            : 'text-base font-semibold mt-2 mb-1';
      switch (level) {
        case 1: blocks.push(<h1 key={key} className={className}>{content}</h1>); break;
        case 2: blocks.push(<h2 key={key} className={className}>{content}</h2>); break;
        case 3: blocks.push(<h3 key={key} className={className}>{content}</h3>); break;
        case 4: blocks.push(<h4 key={key} className={className}>{content}</h4>); break;
        case 5: blocks.push(<h5 key={key} className={className}>{content}</h5>); break;
        default: blocks.push(<h6 key={key} className={className}>{content}</h6>); break;
      }
      i++;
      continue;
    }

    // Unordered list.
    if (isUlLine(line)) {
      const start = i;
      const items: string[] = [];
      while (i < lines.length && isUlLine(lines[i])) {
        items.push(lines[i].replace(/^[-*]\s+/, ''));
        i++;
      }
      blocks.push(
        <ul key={`ul-${start}`} className="list-disc ml-5 my-2 space-y-0.5">
          {items.map((item, k) => (
            <li key={k}>{renderInline(item, `ul-${start}-${k}`)}</li>
          ))}
        </ul>,
      );
      continue;
    }

    // Ordered list.
    if (isOlLine(line)) {
      const start = i;
      const items: string[] = [];
      while (i < lines.length && isOlLine(lines[i])) {
        items.push(lines[i].replace(/^\d+\.\s+/, ''));
        i++;
      }
      blocks.push(
        <ol key={`ol-${start}`} className="list-decimal ml-5 my-2 space-y-0.5">
          {items.map((item, k) => (
            <li key={k}>{renderInline(item, `ol-${start}-${k}`)}</li>
          ))}
        </ol>,
      );
      continue;
    }

    // Blank line.
    if (line.trim() === '') {
      i++;
      continue;
    }

    // Paragraph: consume lines until a blank or a block breakout.
    const paraStart = i;
    const paraLines: string[] = [];
    while (
      i < lines.length &&
      lines[i].trim() !== '' &&
      !isBreakout(lines[i])
    ) {
      paraLines.push(lines[i]);
      i++;
    }
    if (paraLines.length > 0) {
      blocks.push(
        <p key={`p-${paraStart}`} className="my-2 leading-snug whitespace-pre-wrap">
          {renderInline(paraLines.join('\n'), `p-${paraStart}`)}
        </p>,
      );
    }
  }

  return <>{blocks}</>;
}
