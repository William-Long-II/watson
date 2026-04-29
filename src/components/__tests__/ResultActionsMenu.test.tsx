import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { invoke } from '@tauri-apps/api/core';
import { ResultActionsMenu } from '../ResultActionsMenu';
import { useAppStore } from '../../stores/app';
import type { SearchResult } from '../../types';

const mockInvoke = vi.mocked(invoke);

function fileResult(): SearchResult {
  return {
    id: 'f1',
    name: 'config.toml',
    description: '/etc/config.toml',
    icon: null,
    result_type: 'file',
    score: 1,
    action: { type: 'open_file', path: '/etc/config.toml' },
  };
}

function resetStore() {
  useAppStore.setState({
    query: 'config',
    results: [fileResult()],
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
    notifications: [],
    notificationsUnread: 0,
    notificationsOpen: false,
    actionMenuOpen: true,
  });
}

describe('ResultActionsMenu (WAT-404)', () => {
  beforeEach(() => {
    resetStore();
    mockInvoke.mockReset();
    mockInvoke.mockImplementation(async () => undefined);
  });

  it('renders all secondary actions for the result', () => {
    render(<ResultActionsMenu result={fileResult()} />);
    const menu = screen.getByRole('menu', { name: /result actions/i });
    expect(menu).toBeInTheDocument();
    // open_file gets two actions; verify both rendered.
    expect(screen.getByRole('menuitem', { name: /reveal in folder/i })).toBeInTheDocument();
    expect(screen.getByRole('menuitem', { name: /copy path/i })).toBeInTheDocument();
  });

  it('starts with the first action highlighted; ArrowDown moves the highlight', async () => {
    const user = userEvent.setup();
    render(<ResultActionsMenu result={fileResult()} />);

    expect(screen.getByRole('menuitem', { name: /reveal in folder/i })).toHaveAttribute(
      'aria-current',
      'true',
    );

    await user.keyboard('{ArrowDown}');

    expect(screen.getByRole('menuitem', { name: /copy path/i })).toHaveAttribute(
      'aria-current',
      'true',
    );
  });

  it('ArrowUp clamps at the first item', async () => {
    const user = userEvent.setup();
    render(<ResultActionsMenu result={fileResult()} />);
    await user.keyboard('{ArrowUp}'); // already at 0; no change
    expect(screen.getByRole('menuitem', { name: /reveal in folder/i })).toHaveAttribute(
      'aria-current',
      'true',
    );
  });

  it('Enter runs the highlighted action and closes the menu', async () => {
    const user = userEvent.setup();
    render(<ResultActionsMenu result={fileResult()} />);

    await user.keyboard('{Enter}');

    expect(mockInvoke).toHaveBeenCalledWith('reveal_file', { path: '/etc/config.toml' });
    expect(useAppStore.getState().actionMenuOpen).toBe(false);
  });

  it('Esc closes the menu without firing the action', async () => {
    const user = userEvent.setup();
    render(<ResultActionsMenu result={fileResult()} />);

    await user.keyboard('{Escape}');

    expect(mockInvoke).not.toHaveBeenCalled();
    expect(useAppStore.getState().actionMenuOpen).toBe(false);
  });

  it('Cmd+K from inside the menu toggles it closed', async () => {
    const user = userEvent.setup();
    render(<ResultActionsMenu result={fileResult()} />);

    await user.keyboard('{Meta>}k{/Meta}');

    expect(mockInvoke).not.toHaveBeenCalled();
    expect(useAppStore.getState().actionMenuOpen).toBe(false);
  });

  it('clicking an item runs it and closes the menu', async () => {
    const user = userEvent.setup();
    render(<ResultActionsMenu result={fileResult()} />);

    await user.click(screen.getByRole('menuitem', { name: /copy path/i }));

    expect(mockInvoke).toHaveBeenCalledWith('copy_to_clipboard', { content: '/etc/config.toml' });
    expect(useAppStore.getState().actionMenuOpen).toBe(false);
  });
});
