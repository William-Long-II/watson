import { useEffect, useRef, useState } from 'react';
import { useAppStore } from '../stores/app';
import { getSecondaryActions } from '../lib/resultActions';
import type { SearchResult } from '../types';

interface Props {
  result: SearchResult;
}

/**
 * WAT-404: keyboard-driven secondary-action menu for the selected
 * result. Rendered inline (not a popover) so the launcher's existing
 * single-window layout doesn't fight Tauri overlay quirks.
 *
 * Keyboard contract while the menu is open:
 * - ArrowUp / ArrowDown: cycle the menu's selection
 * - Enter: run the selected action and close the menu
 * - Esc / Cmd+K: close without running
 *
 * The menu hides itself when the selected result has no secondary
 * actions — saves users from a confusing empty popover.
 */
export function ResultActionsMenu({ result }: Props) {
  const { closeActionMenu } = useAppStore();
  const actions = getSecondaryActions(result);
  const [activeIndex, setActiveIndex] = useState(0);
  const containerRef = useRef<HTMLDivElement>(null);

  // Reset internal selection whenever the result changes.
  useEffect(() => {
    setActiveIndex(0);
  }, [result.id]);

  // Focus the container so keyboard events route to our handler
  // rather than the search input behind it. SearchBar's keydown
  // handler also forwards Cmd+K and Escape, but local focus avoids
  // the user-input race when the menu is freshly open.
  useEffect(() => {
    containerRef.current?.focus();
  }, []);

  // Empty-action results are filtered at the call site — see
  // ResultsList. Rendering this component implies actions.length > 0.

  const run = async (idx: number) => {
    const action = actions[idx];
    if (!action) return;
    closeActionMenu();
    try {
      await action.run();
    } catch (err) {
      // Best-effort: log and move on. Failures here shouldn't block
      // the user — they're already past the launcher.
      console.error(`Action "${action.label}" failed:`, err);
    }
  };

  const onKeyDown = (e: React.KeyboardEvent) => {
    switch (e.key) {
      case 'ArrowDown':
        e.preventDefault();
        e.stopPropagation();
        setActiveIndex((i) => Math.min(actions.length - 1, i + 1));
        break;
      case 'ArrowUp':
        e.preventDefault();
        e.stopPropagation();
        setActiveIndex((i) => Math.max(0, i - 1));
        break;
      case 'Enter':
        e.preventDefault();
        e.stopPropagation();
        void run(activeIndex);
        break;
      case 'Escape':
        e.preventDefault();
        e.stopPropagation();
        closeActionMenu();
        break;
      case 'k':
      case 'K':
        if (e.metaKey || e.ctrlKey) {
          e.preventDefault();
          e.stopPropagation();
          closeActionMenu();
        }
        break;
    }
  };

  return (
    <div
      ref={containerRef}
      role="menu"
      aria-label="Result actions"
      tabIndex={-1}
      onKeyDown={onKeyDown}
      className="border-t border-[var(--border)] bg-[var(--input-bg)] outline-none"
    >
      {actions.map((action, i) => (
        <button
          key={action.label}
          type="button"
          role="menuitem"
          aria-current={i === activeIndex || undefined}
          onMouseEnter={() => setActiveIndex(i)}
          onClick={() => void run(i)}
          className={`flex items-center gap-2 w-full text-left px-6 py-2 text-xs transition-colors ${
            i === activeIndex
              ? 'bg-blue-500/10 text-[var(--foreground)]'
              : 'text-gray-500 hover:bg-[var(--selected)]'
          }`}
        >
          <svg
            className="w-3.5 h-3.5 shrink-0"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth="2"
            aria-hidden="true"
          >
            <path d="M9 18l6-6-6-6" />
          </svg>
          <span className="truncate">{action.label}</span>
        </button>
      ))}
      <p className="px-6 pb-2 pt-1 text-[10px] text-gray-400 italic">
        Esc to close · Cmd/Ctrl+K to toggle · Enter to run
      </p>
    </div>
  );
}
