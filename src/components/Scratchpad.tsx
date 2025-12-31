import { useEffect, useRef } from 'react';
import { useAppStore } from '../stores/app';

export function Scratchpad() {
  const { scratchpad, saveScratchpad, clearScratchpad, setShowScratchpad, loadScratchpad } = useAppStore();
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    loadScratchpad();
  }, [loadScratchpad]);

  useEffect(() => {
    // Focus textarea when scratchpad opens
    textareaRef.current?.focus();
  }, []);

  const handleChange = (e: React.ChangeEvent<HTMLTextAreaElement>) => {
    saveScratchpad(e.target.value);
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Escape') {
      setShowScratchpad(false);
    }
  };

  return (
    <div className="p-4 border-t border-[var(--border)]">
      <div className="flex justify-between items-center mb-3">
        <h3 className="font-semibold flex items-center gap-2">
          <span className="text-lg">📋</span>
          Scratchpad
        </h3>
        <div className="flex gap-2">
          <button
            onClick={() => clearScratchpad()}
            className="px-2 py-1 text-xs text-gray-500 hover:text-red-500 hover:bg-red-50 dark:hover:bg-red-900/20 rounded transition-colors"
          >
            Clear
          </button>
          <button
            onClick={() => setShowScratchpad(false)}
            className="text-gray-400 hover:text-gray-600"
          >
            <svg className="w-5 h-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <path d="M18 6L6 18M6 6l12 12" />
            </svg>
          </button>
        </div>
      </div>
      <textarea
        ref={textareaRef}
        value={scratchpad}
        onChange={handleChange}
        onKeyDown={handleKeyDown}
        placeholder="Jot something down..."
        className="w-full h-48 p-3 text-sm bg-[var(--input-bg)] border border-[var(--border)] rounded-lg resize-none outline-none focus:ring-1 focus:ring-blue-500"
      />
      <p className="text-xs text-gray-400 mt-2">Press Escape to close. Auto-saves as you type.</p>
    </div>
  );
}
