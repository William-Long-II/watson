import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { Snippet } from '../types';
import { ConfirmModal } from './ConfirmModal';

/**
 * WAT-301: Settings section for managing text snippets.
 *
 * Minimalist CRUD UI: list of (trigger, name) rows with edit/delete;
 * a single inline editor for creating new entries or editing existing
 * ones. Keeps the Settings panel uncluttered — users typically have a
 * handful of snippets at most.
 */
export function SnippetsSettings() {
  const [snippets, setSnippets] = useState<Snippet[]>([]);
  const [editing, setEditing] = useState<Snippet | null>(null);
  const [isAdding, setIsAdding] = useState(false);
  // WAT-405: confirm before deleting a snippet so stray clicks don't
  // wipe an expansion the user has been maintaining for weeks.
  const [deletingSnippet, setDeletingSnippet] = useState<Snippet | null>(null);

  const refresh = async () => {
    try {
      const list = await invoke<Snippet[]>('list_snippets');
      setSnippets(list);
    } catch (e) {
      console.error('Failed to list snippets:', e);
    }
  };

  useEffect(() => {
    refresh();
  }, []);

  const handleSave = async (data: { trigger: string; name: string; expansion: string }) => {
    try {
      if (editing) {
        await invoke<Snippet>('update_snippet', {
          id: editing.id,
          trigger: data.trigger,
          name: data.name,
          expansion: data.expansion,
        });
      } else {
        await invoke<Snippet>('create_snippet', data);
      }
      setEditing(null);
      setIsAdding(false);
      await refresh();
    } catch (e) {
      console.error('Failed to save snippet:', e);
    }
  };

  const requestDelete = (snippet: Snippet) => {
    setDeletingSnippet(snippet);
  };

  const confirmDelete = async () => {
    if (!deletingSnippet) return;
    const id = deletingSnippet.id;
    setDeletingSnippet(null);
    try {
      await invoke('delete_snippet', { id });
      setEditing(null);
      await refresh();
    } catch (e) {
      console.error('Failed to delete snippet:', e);
    }
  };

  return (
    <div>
      <div className="flex justify-between items-center mb-2">
        <label className="text-sm text-gray-500">Snippets</label>
        {!isAdding && editing === null && (
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
        Type your trigger to paste the expansion into whatever app was focused before Watson.
        Convention: prefix triggers with <span className="font-mono text-blue-400">;</span> to
        avoid colliding with app names.
      </p>

      <div className="space-y-2">
        {isAdding && (
          <SnippetEditor
            snippet={null}
            onSave={handleSave}
            onCancel={() => setIsAdding(false)}
          />
        )}

        {snippets.map((s) =>
          editing?.id === s.id ? (
            <SnippetEditor
              key={s.id}
              snippet={s}
              onSave={handleSave}
              onCancel={() => setEditing(null)}
              onDelete={() => requestDelete(s)}
            />
          ) : (
            <div
              key={s.id}
              onClick={() => !isAdding && setEditing(s)}
              className="flex items-center justify-between p-2 bg-[var(--input-bg)] rounded-lg cursor-pointer hover:bg-[var(--selected)] transition-colors"
            >
              <div className="flex items-center gap-2 min-w-0">
                <span className="text-xs font-mono text-blue-400 shrink-0">{s.trigger}</span>
                <span className="text-sm truncate">{s.name}</span>
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

        {snippets.length === 0 && !isAdding && (
          <p className="text-xs text-gray-400 italic">No snippets yet &mdash; add one to get started.</p>
        )}
      </div>

      <ConfirmModal
        open={deletingSnippet !== null}
        title="Delete this snippet?"
        message={
          deletingSnippet
            ? `"${deletingSnippet.trigger}" — ${deletingSnippet.name} — will be removed. This can't be undone.`
            : ''
        }
        confirmLabel="Delete"
        variant="danger"
        onConfirm={confirmDelete}
        onCancel={() => setDeletingSnippet(null)}
      />
    </div>
  );
}

interface SnippetEditorProps {
  snippet: Snippet | null;
  onSave: (data: { trigger: string; name: string; expansion: string }) => void;
  onCancel: () => void;
  onDelete?: () => void;
}

function SnippetEditor({ snippet, onSave, onCancel, onDelete }: SnippetEditorProps) {
  const [trigger, setTrigger] = useState(snippet?.trigger ?? '');
  const [name, setName] = useState(snippet?.name ?? '');
  const [expansion, setExpansion] = useState(snippet?.expansion ?? '');

  const isValid = trigger.trim() && name.trim() && expansion.length > 0;

  return (
    <div className="space-y-2 p-3 bg-[var(--input-bg)] rounded-lg">
      <div>
        <label className="text-xs text-gray-500 mb-1 block">Trigger</label>
        <input
          type="text"
          value={trigger}
          onChange={(e) => setTrigger(e.target.value)}
          placeholder=";addr"
          className="w-full px-3 py-1.5 text-sm font-mono bg-[var(--background)] border border-[var(--border)] rounded-lg outline-none focus:ring-1 focus:ring-blue-500"
        />
      </div>
      <div>
        <label className="text-xs text-gray-500 mb-1 block">Name</label>
        <input
          type="text"
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder="Home Address"
          className="w-full px-3 py-1.5 text-sm bg-[var(--background)] border border-[var(--border)] rounded-lg outline-none focus:ring-1 focus:ring-blue-500"
        />
      </div>
      <div>
        <label className="text-xs text-gray-500 mb-1 block">Expansion</label>
        <textarea
          value={expansion}
          onChange={(e) => setExpansion(e.target.value)}
          placeholder="The text that gets pasted&#10;(multi-line is fine)"
          className="w-full h-20 px-3 py-1.5 text-sm bg-[var(--background)] border border-[var(--border)] rounded-lg resize-y outline-none focus:ring-1 focus:ring-blue-500"
        />
      </div>
      <div className="flex gap-2 pt-1">
        <button
          type="button"
          onClick={() => isValid && onSave({ trigger, name, expansion })}
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
