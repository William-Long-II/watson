import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { invoke } from '@tauri-apps/api/core';
import { NoteEditor } from '../NoteEditor';
import { useAppStore } from '../../stores/app';
import type { Note } from '../../types';

const mockInvoke = vi.mocked(invoke);

function resetStore() {
  useAppStore.setState({
    query: '',
    results: [],
    selectedIndex: 0,
    settings: null,
    isLoading: false,
    currentPanel: null,
    scratchpad: '',
    currentNote: null,
    startupWarnings: [],
    reservedPrefixes: [],
  });
}

function note(overrides: Partial<Note> = {}): Note {
  return {
    id: overrides.id ?? 'note:1',
    title: overrides.title ?? 'Meeting Notes',
    content: overrides.content ?? 'body',
    tags: overrides.tags ?? [],
    created_at: overrides.created_at ?? 1_700_000_000,
    modified_at: overrides.modified_at ?? 1_700_000_000,
    external_changes: overrides.external_changes,
  };
}

describe('NoteEditor — WAT-204 reconcile banner', () => {
  beforeEach(() => {
    resetStore();
    mockInvoke.mockReset();
    mockInvoke.mockImplementation(async () => undefined);
  });

  it('hides the banner when the note is in sync (no external_changes)', () => {
    useAppStore.setState({ currentNote: note() });
    render(<NoteEditor />);
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
  });

  it('renders the banner with both reconcile actions when external_changes is present', () => {
    useAppStore.setState({
      currentNote: note({
        external_changes: {
          disk_title: 'Meeting Notes',
          disk_content: 'edited in vim',
          disk_modified_at: 1_700_001_000,
        },
      }),
    });
    render(<NoteEditor />);

    const alert = screen.getByRole('alert');
    expect(alert).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /use disk version/i })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /keep database version/i })).toBeInTheDocument();
  });

  it('"Use disk version" calls reload_note_from_disk and updates the current note', async () => {
    const reloadedNote = note({
      title: 'Meeting Notes',
      content: 'edited in vim',
      external_changes: undefined, // cleared after reload
    });
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'reload_note_from_disk') return reloadedNote;
      return undefined;
    });

    useAppStore.setState({
      currentNote: note({
        external_changes: {
          disk_title: 'Meeting Notes',
          disk_content: 'edited in vim',
          disk_modified_at: 1_700_001_000,
        },
      }),
    });

    const user = userEvent.setup();
    render(<NoteEditor />);

    await user.click(screen.getByRole('button', { name: /use disk version/i }));

    expect(mockInvoke).toHaveBeenCalledWith('reload_note_from_disk', { id: 'note:1' });
    // After the reload, the in-store note has no external_changes so
    // the banner disappears.
    expect(useAppStore.getState().currentNote?.external_changes).toBeUndefined();
  });

  it('"Keep database version" calls update_note with the editor\'s current buffer', async () => {
    mockInvoke.mockImplementation(async (cmd: string, args: unknown) => {
      if (cmd === 'update_note') {
        const a = args as { id: string; title: string; content: string };
        return {
          id: a.id,
          title: a.title,
          content: a.content,
          tags: [],
          created_at: 1_700_000_000,
          modified_at: 1_700_002_000,
        };
      }
      return undefined;
    });

    useAppStore.setState({
      currentNote: note({
        external_changes: {
          disk_title: 'Meeting Notes',
          disk_content: 'edited in vim',
          disk_modified_at: 1_700_001_000,
        },
      }),
    });

    const user = userEvent.setup();
    render(<NoteEditor />);

    await user.click(screen.getByRole('button', { name: /keep database version/i }));

    // update_note is what the "Keep DB" action calls — with the
    // editor's current (DB-derived) title and content. This also
    // rewrites the disk file, normalizing mtime ≈ modified_at.
    const updateCall = mockInvoke.mock.calls.find((c) => c[0] === 'update_note');
    expect(updateCall, 'update_note should be invoked').toBeDefined();
    const args = updateCall?.[1] as { id: string; title: string; content: string };
    expect(args.id).toBe('note:1');
    expect(args.title).toBe('Meeting Notes');
    expect(args.content).toBe('body');
  });

  // --- WAT-305 markdown preview toggle ---

  it('starts in edit mode with the textarea visible', () => {
    useAppStore.setState({ currentNote: note({ content: '# hello' }) });
    render(<NoteEditor />);
    expect(screen.getByPlaceholderText(/write your note/i)).toBeInTheDocument();
    expect(
      screen.queryByRole('article', { name: /markdown preview/i }),
    ).not.toBeInTheDocument();
  });

  it('toggles to preview mode when the Preview button is clicked', async () => {
    useAppStore.setState({ currentNote: note({ content: '# hello\n\nworld' }) });
    const user = userEvent.setup();
    render(<NoteEditor />);

    await user.click(screen.getByRole('button', { name: /preview/i, pressed: false }));

    // Textarea gone; article + rendered h1 present.
    expect(screen.queryByPlaceholderText(/write your note/i)).not.toBeInTheDocument();
    expect(screen.getByRole('article', { name: /markdown preview/i })).toBeInTheDocument();
    expect(screen.getByRole('heading', { level: 1, name: 'hello' })).toBeInTheDocument();
  });

  it('toggles back to edit when the button is clicked again', async () => {
    useAppStore.setState({ currentNote: note({ content: 'body' }) });
    const user = userEvent.setup();
    render(<NoteEditor />);

    const toggle = screen.getByRole('button', { name: /preview/i });
    await user.click(toggle);
    // Button label flips to "Edit" and aria-pressed is true.
    const backToEdit = screen.getByRole('button', { name: /edit/i, pressed: true });
    await user.click(backToEdit);

    expect(screen.getByPlaceholderText(/write your note/i)).toBeInTheDocument();
    expect(
      screen.queryByRole('article', { name: /markdown preview/i }),
    ).not.toBeInTheDocument();
  });

  it('shows a placeholder in preview mode when the buffer is empty', async () => {
    useAppStore.setState({ currentNote: note({ content: '   \n\n  ' }) });
    const user = userEvent.setup();
    render(<NoteEditor />);
    await user.click(screen.getByRole('button', { name: /preview/i }));
    expect(screen.getByText(/nothing to preview yet/i)).toBeInTheDocument();
  });

  // --- WAT-405: delete-confirmation modal ---

  it('Delete opens the confirm modal and only calls delete_note after Confirm', async () => {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'delete_note') return undefined;
      return undefined;
    });
    useAppStore.setState({ currentNote: note({ id: 'note:9' }) });
    const user = userEvent.setup();
    render(<NoteEditor />);

    await user.click(screen.getByRole('button', { name: /^delete$/i }));

    // Modal is open; backend has NOT been called yet.
    const dialog = screen.getByRole('dialog');
    expect(dialog).toBeInTheDocument();
    const earlyCall = mockInvoke.mock.calls.find((c) => c[0] === 'delete_note');
    expect(earlyCall, 'delete_note must not fire until Confirm is clicked').toBeUndefined();

    // Confirm the delete from inside the modal.
    const modalDelete = Array.from(dialog.querySelectorAll('button'))
      .find((b) => /delete/i.test(b.textContent ?? ''));
    expect(modalDelete).toBeDefined();
    await user.click(modalDelete!);

    expect(mockInvoke).toHaveBeenCalledWith('delete_note', { id: 'note:9' });
  });

  it('Delete modal cancels cleanly (no IPC, no state change)', async () => {
    useAppStore.setState({ currentNote: note({ id: 'note:42' }) });
    const user = userEvent.setup();
    render(<NoteEditor />);

    await user.click(screen.getByRole('button', { name: /^delete$/i }));

    // Press Escape — modal closes, delete never fires.
    await user.keyboard('{Escape}');

    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    const called = mockInvoke.mock.calls.find((c) => c[0] === 'delete_note');
    expect(called).toBeUndefined();
  });
});
