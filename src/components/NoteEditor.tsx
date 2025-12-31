import { useEffect, useRef, useState } from 'react';
import { useAppStore } from '../stores/app';

export function NoteEditor() {
  const { currentNote, createNote, updateNote, deleteNote, closeNoteEditor } = useAppStore();
  const [title, setTitle] = useState('');
  const [content, setContent] = useState('');
  const [isSaving, setIsSaving] = useState(false);
  const titleRef = useRef<HTMLInputElement>(null);
  const contentRef = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    if (currentNote) {
      setTitle(currentNote.title);
      setContent(currentNote.content);
      contentRef.current?.focus();
    } else {
      setTitle('');
      setContent('');
      titleRef.current?.focus();
    }
  }, [currentNote]);

  const handleSave = async () => {
    if (!title.trim()) return;

    setIsSaving(true);
    try {
      if (currentNote) {
        await updateNote(currentNote.id, title, content);
      } else {
        await createNote(title, content);
      }
      closeNoteEditor();
    } finally {
      setIsSaving(false);
    }
  };

  const handleDelete = async () => {
    if (currentNote) {
      await deleteNote(currentNote.id);
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Escape') {
      closeNoteEditor();
    } else if (e.key === 's' && (e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      handleSave();
    }
  };

  return (
    <div className="p-4 border-t border-[var(--border)]" onKeyDown={handleKeyDown}>
      <div className="flex justify-between items-center mb-3">
        <h3 className="font-semibold flex items-center gap-2">
          <span className="text-lg">📝</span>
          {currentNote ? 'Edit Note' : 'New Note'}
        </h3>
        <div className="flex gap-2">
          {currentNote && (
            <button
              onClick={handleDelete}
              className="px-2 py-1 text-xs text-gray-500 hover:text-red-500 hover:bg-red-50 dark:hover:bg-red-900/20 rounded transition-colors"
            >
              Delete
            </button>
          )}
          <button
            onClick={closeNoteEditor}
            className="text-gray-400 hover:text-gray-600"
          >
            <svg className="w-5 h-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <path d="M18 6L6 18M6 6l12 12" />
            </svg>
          </button>
        </div>
      </div>

      <input
        ref={titleRef}
        type="text"
        value={title}
        onChange={(e) => setTitle(e.target.value)}
        placeholder="Note title..."
        className="w-full px-3 py-2 mb-3 text-sm font-medium bg-[var(--input-bg)] border border-[var(--border)] rounded-lg outline-none focus:ring-1 focus:ring-blue-500"
      />

      <textarea
        ref={contentRef}
        value={content}
        onChange={(e) => setContent(e.target.value)}
        placeholder="Write your note... Use #tags to organize"
        className="w-full h-48 p-3 text-sm bg-[var(--input-bg)] border border-[var(--border)] rounded-lg resize-none outline-none focus:ring-1 focus:ring-blue-500"
      />

      <div className="flex justify-between items-center mt-3">
        <p className="text-xs text-gray-400">
          {currentNote ? `Modified ${new Date(currentNote.modified_at * 1000).toLocaleString()}` : 'Tip: Use #hashtags to tag your notes'}
        </p>
        <div className="flex gap-2">
          <button
            onClick={closeNoteEditor}
            className="px-3 py-1.5 text-sm text-gray-600 hover:bg-gray-100 dark:hover:bg-gray-700 rounded transition-colors"
          >
            Cancel
          </button>
          <button
            onClick={handleSave}
            disabled={!title.trim() || isSaving}
            className="px-3 py-1.5 text-sm bg-blue-500 text-white rounded hover:bg-blue-600 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
          >
            {isSaving ? 'Saving...' : 'Save'}
          </button>
        </div>
      </div>
    </div>
  );
}
