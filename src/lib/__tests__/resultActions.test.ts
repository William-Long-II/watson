import { describe, it, expect, beforeEach, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { getSecondaryActions } from '../resultActions';
import type { SearchResult } from '../../types';

const mockInvoke = vi.mocked(invoke);

function r(action: SearchResult['action']): SearchResult {
  return {
    id: 'x',
    name: 'name',
    description: '',
    icon: null,
    result_type: 'application',
    score: 0,
    action,
  };
}

describe('getSecondaryActions (WAT-404)', () => {
  beforeEach(() => {
    mockInvoke.mockReset();
    mockInvoke.mockImplementation(async () => undefined);
  });

  it('launch_app exposes Reveal in folder + Copy path', async () => {
    const actions = getSecondaryActions(r({ type: 'launch_app', path: '/Apps/Foo.app' }));
    expect(actions.map((a) => a.label)).toEqual(['Reveal in folder', 'Copy path']);

    await actions[0].run();
    expect(mockInvoke).toHaveBeenCalledWith('reveal_file', { path: '/Apps/Foo.app' });

    await actions[1].run();
    expect(mockInvoke).toHaveBeenCalledWith('copy_to_clipboard', { content: '/Apps/Foo.app' });
  });

  it('open_file exposes Reveal in folder + Copy path', async () => {
    const actions = getSecondaryActions(r({ type: 'open_file', path: '/tmp/x.txt' }));
    expect(actions.map((a) => a.label)).toEqual(['Reveal in folder', 'Copy path']);

    await actions[0].run();
    expect(mockInvoke).toHaveBeenCalledWith('reveal_file', { path: '/tmp/x.txt' });
  });

  it('open_url exposes Copy URL', async () => {
    const actions = getSecondaryActions(r({ type: 'open_url', url: 'https://example.com/?q=cats' }));
    expect(actions.map((a) => a.label)).toEqual(['Copy URL']);

    await actions[0].run();
    expect(mockInvoke).toHaveBeenCalledWith('copy_to_clipboard', { content: 'https://example.com/?q=cats' });
  });

  it('paste_snippet exposes Copy expansion (no paste)', async () => {
    const actions = getSecondaryActions(r({ type: 'paste_snippet', expansion: 'hello\nworld' }));
    expect(actions.map((a) => a.label)).toEqual(['Copy expansion (no paste)']);

    await actions[0].run();
    expect(mockInvoke).toHaveBeenCalledWith('copy_to_clipboard', { content: 'hello\nworld' });
  });

  it('returns empty for action types where the primary already covers what we want', () => {
    // Pin: copy_clipboard primary is already "copy"; run_command runs the
    // command (no secondary makes sense); open_note opens the editor.
    expect(getSecondaryActions(r({ type: 'copy_clipboard', content: 'x' }))).toEqual([]);
    expect(getSecondaryActions(r({ type: 'run_command', command: 'cmd:lock' }))).toEqual([]);
    expect(getSecondaryActions(r({ type: 'open_note', note_id: 'note:1' }))).toEqual([]);
  });
});
