import { useEffect, useRef } from 'react';
import { useAppStore } from '../stores/app';
import { getSecondaryActions } from '../lib/resultActions';

export function SearchBar() {
  const inputRef = useRef<HTMLInputElement>(null);
  const {
    query,
    setQuery,
    moveSelection,
    executeSelected,
    hideWindow,
    setShowScratchpad,
    openNewNote,
    actionMenuOpen,
    toggleActionMenu,
    closeActionMenu,
    results,
    selectedIndex,
  } = useAppStore();
  // WAT-402: aria-activedescendant points to the currently-highlighted
  // result row id so screen readers track keyboard navigation through
  // the listbox without the input losing focus.
  const activeOptionId = results[selectedIndex]
    ? `result-${results[selectedIndex].id}`
    : undefined;

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  const handleKeyDown = (e: React.KeyboardEvent) => {
    // WAT-404: Cmd/Ctrl+K toggles the per-result actions menu. Handled
    // here at the SearchBar level (the input owns focus) but only opens
    // when the currently-selected result actually has secondary actions
    // — otherwise the toggle is a no-op so users never see an empty
    // menu.
    if ((e.metaKey || e.ctrlKey) && (e.key === 'k' || e.key === 'K')) {
      e.preventDefault();
      const { results, selectedIndex } = useAppStore.getState();
      const selected = results[selectedIndex];
      if (selected && getSecondaryActions(selected).length > 0) {
        toggleActionMenu();
      }
      return;
    }

    // When the actions menu is open, the menu component owns most key
    // handling. SearchBar still handles Escape as a fallback so the
    // user can dismiss without focusing the menu.
    if (actionMenuOpen) {
      if (e.key === 'Escape') {
        e.preventDefault();
        closeActionMenu();
        return;
      }
      // Forward navigation/Enter to the menu — let it bubble naturally
      // by NOT preventing default here. The menu's onKeyDown sees them.
      return;
    }

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
          aria-label="Search"
          role="combobox"
          aria-controls="search-results-listbox"
          aria-expanded={results.length > 0}
          aria-autocomplete="list"
          aria-activedescendant={activeOptionId}
          autoComplete="off"
          spellCheck={false}
          className="w-full pl-12 pr-4 py-3 text-base bg-[var(--input-bg)] text-[var(--foreground)] rounded-xl outline-none focus:ring-2 focus:ring-blue-500/50 transition-all placeholder:text-gray-400"
          autoFocus
        />
      </div>
    </div>
  );
}
