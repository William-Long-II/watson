import { useEffect, useRef } from 'react';
import { useAppStore } from '../stores/app';

export function SearchBar() {
  const inputRef = useRef<HTMLInputElement>(null);
  const { query, setQuery, moveSelection, executeSelected, hideWindow, setShowScratchpad, openNewNote } = useAppStore();

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  const handleKeyDown = (e: React.KeyboardEvent) => {
    // Chained shortcuts when search is empty
    if (query === '') {
      // Scratchpad trigger: 's' or '`'
      if (e.key === 's' || e.key === '`') {
        e.preventDefault();
        setShowScratchpad(true);
        return;
      }
      // New note trigger: 'n' (shift+n for search mode)
      if (e.key === 'n' && !e.shiftKey) {
        e.preventDefault();
        openNewNote();
        return;
      }
      // Notes search: 'N' (shift+n)
      if (e.key === 'N' || (e.key === 'n' && e.shiftKey)) {
        e.preventDefault();
        setQuery('n ');
        return;
      }
      // Files search: 'f'
      if (e.key === 'f') {
        e.preventDefault();
        setQuery('f ');
        return;
      }
    }

    switch (e.key) {
      case 'ArrowDown':
        e.preventDefault();
        moveSelection(1);
        break;
      case 'ArrowUp':
        e.preventDefault();
        moveSelection(-1);
        break;
      case 'Enter':
        e.preventDefault();
        executeSelected();
        break;
      case 'Escape':
        e.preventDefault();
        if (query) {
          setQuery('');
        } else {
          hideWindow();
        }
        break;
      case 'Tab':
        e.preventDefault();
        moveSelection(e.shiftKey ? -1 : 1);
        break;
    }
  };

  return (
    <div className="p-4">
      <div className="relative">
        <svg
          className="absolute left-4 top-1/2 -translate-y-1/2 w-5 h-5 text-gray-400"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
        >
          <circle cx="11" cy="11" r="8" />
          <path d="m21 21-4.3-4.3" />
        </svg>
        <input
          ref={inputRef}
          type="text"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={handleKeyDown}
          placeholder="Search apps, commands, or type a keyword..."
          className="w-full pl-12 pr-4 py-3 text-base bg-[var(--input-bg)] text-[var(--foreground)] rounded-xl outline-none focus:ring-2 focus:ring-blue-500/50 transition-all placeholder:text-gray-400"
          autoFocus
        />
      </div>
    </div>
  );
}
