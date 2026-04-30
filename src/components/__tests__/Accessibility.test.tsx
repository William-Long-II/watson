import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { SearchBar } from '../SearchBar';
import { ResultsList } from '../ResultsList';
import { useAppStore } from '../../stores/app';
import type { SearchResult } from '../../types';

const mockInvoke = vi.mocked(invoke);

function makeResult(overrides: Partial<SearchResult> = {}): SearchResult {
  return {
    id: overrides.id ?? 'r1',
    name: overrides.name ?? 'Result 1',
    description: overrides.description ?? '',
    icon: null,
    result_type: overrides.result_type ?? 'application',
    score: 0,
    action: { type: 'launch_app', path: '/bin/true' },
  } as SearchResult;
}

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
    notifications: [],
    notificationsUnread: 0,
    notificationsOpen: false,
    actionMenuOpen: false,
  });
}

describe('Accessibility plumbing (WAT-402)', () => {
  beforeEach(() => {
    resetStore();
    mockInvoke.mockReset();
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'search') return [];
      return undefined;
    });
  });

  // --- combobox / listbox ---

  it('SearchBar input is a combobox with autocomplete=list', () => {
    render(<SearchBar />);
    const input = screen.getByRole('combobox', { name: /search/i });
    expect(input).toHaveAttribute('aria-autocomplete', 'list');
    expect(input).toHaveAttribute('aria-controls', 'search-results-listbox');
  });

  it('combobox aria-expanded reflects whether there are results', () => {
    useAppStore.setState({ results: [] });
    const { rerender } = render(<SearchBar />);
    expect(screen.getByRole('combobox')).toHaveAttribute('aria-expanded', 'false');

    useAppStore.setState({ query: 'x', results: [makeResult()] });
    rerender(<SearchBar />);
    expect(screen.getByRole('combobox')).toHaveAttribute('aria-expanded', 'true');
  });

  it('aria-activedescendant tracks the selected result', () => {
    useAppStore.setState({
      query: 'x',
      results: [makeResult({ id: 'a' }), makeResult({ id: 'b' })],
      selectedIndex: 0,
    });
    const { rerender } = render(<SearchBar />);
    expect(screen.getByRole('combobox')).toHaveAttribute('aria-activedescendant', 'result-a');

    useAppStore.setState({ selectedIndex: 1 });
    rerender(<SearchBar />);
    expect(screen.getByRole('combobox')).toHaveAttribute('aria-activedescendant', 'result-b');
  });

  it('aria-activedescendant is unset when there are no results', () => {
    useAppStore.setState({ query: 'x', results: [] });
    render(<SearchBar />);
    const input = screen.getByRole('combobox');
    expect(input).not.toHaveAttribute('aria-activedescendant');
  });

  it('ResultsList renders as a labelled listbox', () => {
    useAppStore.setState({
      query: 'x',
      results: [makeResult({ id: 'a' })],
    });
    render(<ResultsList />);
    const listbox = screen.getByRole('listbox', { name: /search results/i });
    expect(listbox).toBeInTheDocument();
    expect(listbox).toHaveAttribute('id', 'search-results-listbox');
  });

  it('listbox is labelled "Recent items" when query is empty', () => {
    useAppStore.setState({ query: '', results: [makeResult()] });
    render(<ResultsList />);
    expect(screen.getByRole('listbox', { name: /recent items/i })).toBeInTheDocument();
  });

  it('result rows are options with aria-selected reflecting the active row', () => {
    useAppStore.setState({
      query: 'x',
      results: [makeResult({ id: 'a' }), makeResult({ id: 'b' })],
      selectedIndex: 1,
    });
    render(<ResultsList />);
    const options = screen.getAllByRole('option');
    expect(options).toHaveLength(2);
    expect(options[0]).toHaveAttribute('aria-selected', 'false');
    expect(options[1]).toHaveAttribute('aria-selected', 'true');
  });

  it('result option ids match the activedescendant pointer', () => {
    useAppStore.setState({
      query: 'x',
      results: [makeResult({ id: 'a' })],
      selectedIndex: 0,
    });
    render(<ResultsList />);
    const opt = screen.getByRole('option');
    expect(opt).toHaveAttribute('id', 'result-a');
  });

  it('result option exposes a useful accessible name combining type, name, and description', () => {
    useAppStore.setState({
      query: 'x',
      results: [
        makeResult({
          id: 'r1',
          name: 'Chrome',
          description: 'Web browser',
          result_type: 'application',
        }),
      ],
    });
    render(<ResultsList />);
    // The accessible name comes from the explicit aria-label we set on
    // the option; it should mention the type, name, and description so
    // a screen reader announces all three when the row gains focus.
    const opt = screen.getByRole('option');
    const label = opt.getAttribute('aria-label') ?? '';
    expect(label).toContain('Chrome');
    expect(label).toContain('Web browser');
    expect(label).toContain('App');
  });
});
