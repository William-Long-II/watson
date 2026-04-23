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
    showSettings: false,
    scratchpad: '',
    scratchpadVisible: false,
    currentNote: null,
    noteEditorVisible: false,
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
});
