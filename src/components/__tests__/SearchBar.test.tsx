import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { invoke } from '@tauri-apps/api/core';
import { SearchBar } from '../SearchBar';
import { useAppStore } from '../../stores/app';
import type { SearchResult } from '../../types';

const mockInvoke = vi.mocked(invoke);

/**
 * Reset the zustand store to a clean baseline before every test. Without
 * this, state mutations from one test leak into the next — zustand stores
 * are module-level singletons.
 */
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
  });
}

function makeResult(overrides: Partial<SearchResult> = {}): SearchResult {
  return {
    id: overrides.id ?? 'r1',
    name: overrides.name ?? 'Result 1',
    description: overrides.description ?? '',
    icon: overrides.icon ?? null,
    result_type: overrides.result_type ?? 'application',
    score: overrides.score ?? 100,
    action: overrides.action ?? { type: 'launch_app', path: '/bin/true' },
  } as SearchResult;
}

/**
 * Seed the store with a set of results so selection / Enter tests have
 * something to act on.
 */
function seedResults(results: SearchResult[]) {
  useAppStore.setState({ results, selectedIndex: 0 });
}

describe('SearchBar keyboard contract', () => {
  beforeEach(() => {
    resetStore();
    mockInvoke.mockReset();
    // Default invoke: return empty result list so setQuery doesn't blow up.
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'search') return [];
      return undefined;
    });
  });

  // --- focus ---

  it('focuses the input on mount', () => {
    render(<SearchBar />);
    const input = screen.getByPlaceholderText(/search apps/i);
    expect(input).toHaveFocus();
  });

  // --- arrow-key navigation ---

  it('ArrowDown moves selection forward within results', async () => {
    const user = userEvent.setup();
    seedResults([makeResult({ id: 'a' }), makeResult({ id: 'b' }), makeResult({ id: 'c' })]);
    render(<SearchBar />);

    await user.keyboard('{ArrowDown}');
    expect(useAppStore.getState().selectedIndex).toBe(1);

    await user.keyboard('{ArrowDown}');
    expect(useAppStore.getState().selectedIndex).toBe(2);
  });

  it('ArrowDown clamps at last result — does not wrap', async () => {
    const user = userEvent.setup();
    seedResults([makeResult({ id: 'a' }), makeResult({ id: 'b' })]);
    useAppStore.setState({ selectedIndex: 1 });
    render(<SearchBar />);

    await user.keyboard('{ArrowDown}');
    expect(useAppStore.getState().selectedIndex).toBe(1);
  });

  it('ArrowUp moves selection backward and clamps at 0', async () => {
    const user = userEvent.setup();
    seedResults([makeResult({ id: 'a' }), makeResult({ id: 'b' })]);
    useAppStore.setState({ selectedIndex: 1 });
    render(<SearchBar />);

    await user.keyboard('{ArrowUp}');
    expect(useAppStore.getState().selectedIndex).toBe(0);

    await user.keyboard('{ArrowUp}');
    expect(useAppStore.getState().selectedIndex).toBe(0);
  });

  // --- Tab / Shift+Tab — matches v1.2.1 behavior ---

  it('Tab cycles selection forward (same as ArrowDown)', async () => {
    const user = userEvent.setup();
    seedResults([makeResult({ id: 'a' }), makeResult({ id: 'b' })]);
    render(<SearchBar />);

    await user.keyboard('{Tab}');
    expect(useAppStore.getState().selectedIndex).toBe(1);
  });

  it('Shift+Tab cycles selection backward', async () => {
    const user = userEvent.setup();
    seedResults([makeResult({ id: 'a' }), makeResult({ id: 'b' })]);
    useAppStore.setState({ selectedIndex: 1 });
    render(<SearchBar />);

    await user.keyboard('{Shift>}{Tab}{/Shift}');
    expect(useAppStore.getState().selectedIndex).toBe(0);
  });

  // --- Enter / Escape ---

  it('Enter invokes execute_action for the selected result', async () => {
    const user = userEvent.setup();
    const action = { type: 'launch_app' as const, path: '/bin/chrome' };
    seedResults([makeResult({ id: 'chrome', action })]);
    render(<SearchBar />);

    await user.keyboard('{Enter}');

    // executeSelected clears query and calls invoke('execute_action', ...).
    // The hide_window call also happens; we only assert the execute_action one.
    const executeCall = mockInvoke.mock.calls.find((c) => c[0] === 'execute_action');
    expect(executeCall, 'execute_action should be invoked').toBeDefined();
    expect(executeCall?.[1]).toEqual({ action });
  });

  it('Escape clears a non-empty query without hiding the window', async () => {
    const user = userEvent.setup();
    useAppStore.setState({ query: 'chrome' });
    render(<SearchBar />);

    await user.keyboard('{Escape}');

    expect(useAppStore.getState().query).toBe('');
    const hideCall = mockInvoke.mock.calls.find((c) => c[0] === 'hide_window');
    expect(hideCall, 'hide_window should NOT be called when query was non-empty').toBeUndefined();
  });

  it('Escape on an empty query hides the window', async () => {
    const user = userEvent.setup();
    render(<SearchBar />);

    await user.keyboard('{Escape}');

    const hideCall = mockInvoke.mock.calls.find((c) => c[0] === 'hide_window');
    expect(hideCall, 'hide_window should be called when query was empty').toBeDefined();
  });

  // --- empty-query chained shortcuts ---

  it("typing 's' on an empty query opens the scratchpad", async () => {
    const user = userEvent.setup();
    render(<SearchBar />);

    await user.keyboard('s');

    expect(useAppStore.getState().scratchpadVisible).toBe(true);
    // The 's' character must NOT have been inserted into the query.
    expect(useAppStore.getState().query).toBe('');
  });

  it('typing backtick on an empty query opens the scratchpad', async () => {
    const user = userEvent.setup();
    render(<SearchBar />);

    await user.keyboard('`');

    expect(useAppStore.getState().scratchpadVisible).toBe(true);
    expect(useAppStore.getState().query).toBe('');
  });

  it("typing 'n' on an empty query opens a new note", async () => {
    const user = userEvent.setup();
    render(<SearchBar />);

    await user.keyboard('n');

    expect(useAppStore.getState().noteEditorVisible).toBe(true);
    expect(useAppStore.getState().currentNote).toBeNull();
    expect(useAppStore.getState().query).toBe('');
  });

  it("Shift+n on an empty query enters notes-search mode ('n ')", async () => {
    const user = userEvent.setup();
    render(<SearchBar />);

    await user.keyboard('{Shift>}n{/Shift}');

    expect(useAppStore.getState().query).toBe('n ');
    expect(useAppStore.getState().noteEditorVisible).toBe(false);
  });

  it("typing 'f' on an empty query enters files-search mode ('f ')", async () => {
    const user = userEvent.setup();
    render(<SearchBar />);

    await user.keyboard('f');

    expect(useAppStore.getState().query).toBe('f ');
  });

  // --- shortcut suppression once the query has content ---

  it("'s' is typed normally once the query is non-empty", async () => {
    const user = userEvent.setup();
    useAppStore.setState({ query: 'a' });
    render(<SearchBar />);

    const input = screen.getByPlaceholderText(/search apps/i);
    input.focus();
    await user.keyboard('s');

    // The 's' reaches the input and triggers a search; scratchpad must not open.
    expect(useAppStore.getState().scratchpadVisible).toBe(false);
  });
});
