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
    currentPanel: null,
    scratchpad: '',
    currentNote: null,
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

  // --- empty-query shortcuts (post-letter+space rework) ---
  //
  // Bare-letter shortcuts used to fire on a single keystroke from
  // empty: `s` → scratchpad, `n` → new note, `N` → notes search,
  // `f` → files search. They were removed because they ate any
  // search starting with that letter (a tab named "Slack" was
  // unreachable; typing "Notion" opened the new-note editor).
  //
  // Now: bare letters flow into the query like normal. Discriminated
  // shortcuts:
  //   `        → scratchpad (single-key OK; rare in real searches)
  //   's '     → scratchpad (typed two chars; intercepted in store)
  //   'n '     → notes search (existing backend route, unchanged)
  //   'f '     → files search (existing backend route, unchanged)

  it('typing backtick on an empty query opens the scratchpad', async () => {
    const user = userEvent.setup();
    render(<SearchBar />);

    await user.keyboard('`');

    expect(useAppStore.getState().currentPanel).toBe('scratchpad');
    expect(useAppStore.getState().query).toBe('');
  });

  it("typing 's' alone leaves it as part of the search query", async () => {
    const user = userEvent.setup();
    render(<SearchBar />);

    await user.keyboard('s');

    // 's' is now a normal search character; scratchpad does NOT open.
    expect(useAppStore.getState().currentPanel).not.toBe('scratchpad');
    expect(useAppStore.getState().query).toBe('s');
  });

  it("'s ' (s + space) opens the scratchpad and clears the query", async () => {
    const user = userEvent.setup();
    render(<SearchBar />);

    await user.keyboard('s ');

    // After the space, the store's setQuery interceptor fires the
    // shortcut and clears the input — same end state as the old
    // single-key trigger, just discriminated.
    expect(useAppStore.getState().currentPanel).toBe('scratchpad');
    expect(useAppStore.getState().query).toBe('');
  });

  it("typing 'n' alone leaves it as part of the search query (no editor)", async () => {
    const user = userEvent.setup();
    render(<SearchBar />);

    await user.keyboard('n');

    // 'n' no longer opens the new note editor — flows into the query.
    expect(useAppStore.getState().currentPanel).not.toBe('noteEditor');
    expect(useAppStore.getState().query).toBe('n');
  });

  it("typing 'f' alone leaves it as part of the search query", async () => {
    const user = userEvent.setup();
    render(<SearchBar />);

    await user.keyboard('f');

    // 'f' no longer auto-writes 'f '; user types the space themselves
    // when they want files-search mode.
    expect(useAppStore.getState().query).toBe('f');
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
    expect(useAppStore.getState().currentPanel).not.toBe('scratchpad');
  });
});
