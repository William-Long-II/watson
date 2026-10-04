import { useState } from 'react';
import { useAppStore } from '../stores/app';
import type { ScriptCommand } from '../types';
import { ConfirmModal } from './ConfirmModal';

/**
 * WAT-501: Settings section for script commands. Each entry binds a
 * keyword to a script; typing `<keyword> <query>` runs the script and
 * shows its JSON output as results. Stored in `settings.script_commands`,
 * so saving goes through the regular settings save path.
 */
export function ScriptCommandsSettings() {
  const { settings, saveSettings, reservedPrefixes } = useAppStore();
  const [editingIndex, setEditingIndex] = useState<number | null>(null);
  const [isAdding, setIsAdding] = useState(false);
  const [deletingIndex, setDeletingIndex] = useState<number | null>(null);

  if (!settings) return null;
  const commands = settings.script_commands ?? [];

  const takenKeywords = (exceptIndex: number | null) => [
    ...reservedPrefixes,
    ...settings.web_searches.map((ws) => ws.keyword),
    ...commands.filter((_, i) => i !== exceptIndex).map((c) => c.keyword),
  ];

  const handleSave = (cmd: ScriptCommand, index: number | null) => {
    const next = [...commands];
    if (index !== null) {
      next[index] = cmd;
    } else {
      next.push(cmd);
    }
    saveSettings({ ...settings, script_commands: next });
    setEditingIndex(null);
    setIsAdding(false);
  };

  const confirmDelete = () => {
    if (deletingIndex === null) return;
    const next = commands.filter((_, i) => i !== deletingIndex);
    setDeletingIndex(null);
    setEditingIndex(null);
    saveSettings({ ...settings, script_commands: next });
  };

  return (
    <div>
      <div className="flex justify-between items-center mb-2">
        <label className="text-sm text-gray-500">Script Commands</label>
        {!isAdding && editingIndex === null && (
          <button
            type="button"
            onClick={() => setIsAdding(true)}
            className="text-xs text-blue-500 hover:text-blue-600"
          >
            + Add New
          </button>
        )}
      </div>
      <p className="text-xs text-gray-400 mb-2">
        Type the keyword, a space, and your query. Watson runs the script with the query as its first
        argument and shows the JSON it prints as results. Scripts run with your own permissions, so
        only add scripts you trust. See the README for the output format.
      </p>

      <div className="space-y-2">
        {isAdding && (
          <ScriptCommandEditor
            command={null}
            takenKeywords={takenKeywords(null)}
            onSave={(cmd) => handleSave(cmd, null)}
            onCancel={() => setIsAdding(false)}
          />
        )}

        {commands.map((c, index) =>
          editingIndex === index ? (
            <ScriptCommandEditor
              key={`${c.keyword}-${index}`}
              command={c}
              takenKeywords={takenKeywords(index)}
              onSave={(cmd) => handleSave(cmd, index)}
              onCancel={() => setEditingIndex(null)}
              onDelete={() => setDeletingIndex(index)}
            />
          ) : (
            <div
              key={`${c.keyword}-${index}`}
              onClick={() => !isAdding && setEditingIndex(index)}
              className="flex items-center justify-between p-2 bg-[var(--input-bg)] rounded-lg cursor-pointer hover:bg-[var(--selected)] transition-colors"
            >
              <div className="flex items-center gap-2 min-w-0">
                {c.icon && <span aria-hidden="true">{c.icon}</span>}
                <span className="font-medium text-sm truncate">{c.name}</span>
                <span className="text-xs font-mono text-blue-400 shrink-0">({c.keyword})</span>
              </div>
              <svg
                className="w-4 h-4 text-gray-400 shrink-0"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                strokeWidth="2"
              >
                <path d="M9 18l6-6-6-6" />
              </svg>
            </div>
          ),
        )}

        {commands.length === 0 && !isAdding && (
          <p className="text-xs text-gray-400 italic">No script commands yet &mdash; add one to get started.</p>
        )}
      </div>

      <ConfirmModal
        open={deletingIndex !== null}
        title="Delete this script command?"
        message={
          deletingIndex !== null && commands[deletingIndex]
            ? `"${commands[deletingIndex].keyword}" — ${commands[deletingIndex].name} — will be removed. The script file itself is not deleted.`
            : ''
        }
        confirmLabel="Delete"
        variant="danger"
        onConfirm={confirmDelete}
        onCancel={() => setDeletingIndex(null)}
      />
    </div>
  );
}

interface ScriptCommandEditorProps {
  command: ScriptCommand | null;
  takenKeywords: string[];
  onSave: (cmd: ScriptCommand) => void;
  onCancel: () => void;
  onDelete?: () => void;
}

const inputClass =
  'w-full px-3 py-1.5 text-sm bg-[var(--background)] border border-[var(--border)] rounded-lg outline-none focus:ring-1 focus:ring-blue-500';

function ScriptCommandEditor({ command, takenKeywords, onSave, onCancel, onDelete }: ScriptCommandEditorProps) {
  const [name, setName] = useState(command?.name ?? '');
  const [keyword, setKeyword] = useState(command?.keyword ?? '');
  const [script, setScript] = useState(command?.script ?? '');
  const [interpreter, setInterpreter] = useState(command?.interpreter ?? '');
  const [icon, setIcon] = useState(command?.icon ?? '');

  const trimmedKeyword = keyword.trim();
  const keywordHasSpace = /\s/.test(trimmedKeyword);
  // Don't block save on a collision (same policy as web searches), just
  // say which one wins: script commands route before everything else.
  const collides = trimmedKeyword !== '' && takenKeywords.includes(trimmedKeyword);
  const isValid = name.trim() !== '' && trimmedKeyword !== '' && !keywordHasSpace && script.trim() !== '';

  const handleSave = () => {
    if (!isValid) return;
    onSave({
      name: name.trim(),
      keyword: trimmedKeyword,
      script: script.trim(),
      interpreter: interpreter.trim() || undefined,
      icon: icon.trim() || undefined,
      timeout_ms: command?.timeout_ms,
    });
  };

  return (
    <div className="space-y-2 p-3 bg-[var(--input-bg)] rounded-lg">
      <div>
        <label htmlFor="script-name" className="text-xs text-gray-500 mb-1 block">Name</label>
        <input
          id="script-name"
          type="text"
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder="Weather"
          className={inputClass}
        />
      </div>
      <div>
        <label htmlFor="script-keyword" className="text-xs text-gray-500 mb-1 block">Keyword</label>
        <input
          id="script-keyword"
          type="text"
          value={keyword}
          onChange={(e) => setKeyword(e.target.value)}
          placeholder="w"
          className={`${inputClass} font-mono`}
        />
        {keywordHasSpace && (
          <p role="alert" className="text-xs text-red-500 mt-1">
            Keywords can't contain spaces.
          </p>
        )}
        {collides && (
          <p role="alert" className="text-xs text-amber-600 dark:text-amber-400 mt-1">
            "{trimmedKeyword}" is already used by Watson, a web search, or another script. This script
            command will take precedence when you type "{trimmedKeyword} " followed by a query.
          </p>
        )}
      </div>
      <div>
        <label htmlFor="script-path" className="text-xs text-gray-500 mb-1 block">Script path</label>
        <input
          id="script-path"
          type="text"
          value={script}
          onChange={(e) => setScript(e.target.value)}
          placeholder="~/scripts/weather.py"
          className={`${inputClass} font-mono`}
        />
      </div>
      <div>
        <label htmlFor="script-interpreter" className="text-xs text-gray-500 mb-1 block">
          Interpreter (optional; inferred from .py, .js, .sh, .rb, .ps1)
        </label>
        <input
          id="script-interpreter"
          type="text"
          value={interpreter}
          onChange={(e) => setInterpreter(e.target.value)}
          placeholder="/opt/homebrew/bin/node"
          className={`${inputClass} font-mono`}
        />
      </div>
      <div>
        <label htmlFor="script-icon" className="text-xs text-gray-500 mb-1 block">Icon (optional emoji)</label>
        <input
          id="script-icon"
          type="text"
          value={icon}
          onChange={(e) => setIcon(e.target.value)}
          placeholder="🌤"
          className={inputClass}
        />
      </div>
      <div className="flex gap-2 pt-1">
        <button
          type="button"
          onClick={handleSave}
          disabled={!isValid}
          className="px-3 py-1.5 text-sm bg-blue-500 text-white rounded-lg hover:bg-blue-600 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
        >
          Save
        </button>
        <button
          type="button"
          onClick={onCancel}
          className="px-3 py-1.5 text-sm bg-[var(--selected)] rounded-lg hover:bg-[var(--border)] transition-colors"
        >
          Cancel
        </button>
        {onDelete && (
          <button
            type="button"
            onClick={onDelete}
            className="px-3 py-1.5 text-sm text-red-500 hover:bg-red-50 dark:hover:bg-red-900/20 rounded-lg transition-colors ml-auto"
          >
            Delete
          </button>
        )}
      </div>
    </div>
  );
}
