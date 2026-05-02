import { useAppStore } from '../stores/app';
import { ResultItem } from './ResultItem';
import { ResultActionsMenu } from './ResultActionsMenu';
import { getSecondaryActions } from '../lib/resultActions';

function EmptyState() {
  return (
    <div className="px-4 py-4 text-center text-sm text-gray-400 animate-fade-in">
      <div className="space-y-1">
        <p className="font-medium text-gray-500 text-xs">Quick Tips</p>
        <div className="space-y-0.5 text-xs">
          <p><span className="text-blue-400 font-mono">g query</span> - Google search</p>
          <p><span className="text-blue-400 font-mono">n&nbsp;</span> - Notes • <span className="text-blue-400 font-mono">f&nbsp;</span> - Files</p>
          <p><span className="text-blue-400 font-mono">cb&nbsp;</span> - Clipboard • <span className="text-blue-400 font-mono">`</span> - Scratchpad</p>
        </div>
      </div>
    </div>
  );
}

export function ResultsList() {
  const { results, selectedIndex, setSelectedIndex, executeSelected, query, actionMenuOpen } =
    useAppStore();

  // WAT-304: on an empty query, the backend returns recents (up to 5
  // apps + 5 files). Show them with a "Recents" header. When recents
  // are empty too — fresh install, no history yet — fall back to the
  // quick-tips EmptyState.
  const queryEmpty = !query.trim();
  if (queryEmpty && results.length === 0) {
    return <EmptyState />;
  }

  // Non-empty query with no results — stay silent while the user is
  // still typing; the last results render via the map below.
  if (results.length === 0) {
    return null;
  }

  return (
    // WAT-402: listbox pattern. The SearchBar input has role="combobox"
    // and points its `aria-controls` at this list's id so screen readers
    // can announce result-count changes and the active descendant as the
    // user navigates with arrow keys.
    <div
      id="search-results-listbox"
      role="listbox"
      aria-label={queryEmpty ? 'Recent items' : 'Search results'}
      className="border-t border-[var(--border)] max-h-[320px] overflow-y-auto"
    >
      {queryEmpty && (
        <p
          data-testid="recents-header"
          className="px-4 py-1.5 text-[10px] uppercase tracking-wider text-gray-400 font-medium"
        >
          Recents
        </p>
      )}
      {results.map((result, index) => (
        <div key={result.id}>
          <ResultItem
            result={result}
            isSelected={index === selectedIndex}
            index={index}
            onClick={() => {
              setSelectedIndex(index);
              executeSelected();
            }}
          />
          {/* WAT-404: render the secondary-actions menu inline below the
              currently-selected row. We re-check `getSecondaryActions`
              here so a result with no secondaries hides the menu even
              if `actionMenuOpen` is somehow stuck true. */}
          {actionMenuOpen &&
            index === selectedIndex &&
            getSecondaryActions(result).length > 0 && <ResultActionsMenu result={result} />}
        </div>
      ))}
    </div>
  );
}
