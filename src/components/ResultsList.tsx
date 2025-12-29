import { useAppStore } from '../stores/app';
import { ResultItem } from './ResultItem';

function EmptyState() {
  return (
    <div className="px-4 py-4 text-center text-sm text-gray-400 animate-fade-in">
      <div className="space-y-1">
        <p className="font-medium text-gray-500 text-xs">Quick Tips</p>
        <div className="space-y-0.5 text-xs">
          <p><span className="text-blue-400 font-mono">g query</span> - Google search</p>
          <p><span className="text-blue-400 font-mono">cb</span> - Clipboard history</p>
          <p><span className="text-blue-400 font-mono">&gt; command</span> - System commands</p>
        </div>
      </div>
    </div>
  );
}

export function ResultsList() {
  const { results, selectedIndex, setSelectedIndex, executeSelected, query } = useAppStore();

  // Show empty state when no query
  if (!query.trim()) {
    return <EmptyState />;
  }

  // Show nothing while searching (will show results when ready)
  if (results.length === 0) {
    return null;
  }

  return (
    <div className="border-t border-[var(--border)] max-h-[320px] overflow-y-auto">
      {results.map((result, index) => (
        <ResultItem
          key={result.id}
          result={result}
          isSelected={index === selectedIndex}
          index={index}
          onClick={() => {
            setSelectedIndex(index);
            executeSelected();
          }}
        />
      ))}
    </div>
  );
}
