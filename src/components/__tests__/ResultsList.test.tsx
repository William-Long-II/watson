import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { ResultsList } from '../ResultsList';
import { useAppStore } from '../../stores/app';
import type { SearchResult } from '../../types';

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

function appResult(id: string, name: string): SearchResult {
  return {
    id,
    name,
    description: 'Recently launched',
    icon: null,
    result_type: 'application',
    score: 10000,
    action: { type: 'launch_app', path: `/bin/${name.toLowerCase()}` },
  } as SearchResult;
}

describe('ResultsList — WAT-304 recents on empty query', () => {
  beforeEach(() => {
    resetStore();
    mockInvoke.mockReset();
    mockInvoke.mockImplementation(async () => undefined);
  });

  it('shows the quick-tips empty state when query and results are both empty', () => {
    render(<ResultsList />);
    expect(screen.getByText(/quick tips/i)).toBeInTheDocument();
    expect(screen.queryByTestId('recents-header')).not.toBeInTheDocument();
  });

  it('shows a "Recents" header above results when the query is empty but recents exist', () => {
    useAppStore.setState({
      query: '',
      results: [appResult('app:chrome', 'Chrome'), appResult('app:vscode', 'VS Code')],
    });
    render(<ResultsList />);

    expect(screen.getByTestId('recents-header')).toBeInTheDocument();
    expect(screen.getByText('Chrome')).toBeInTheDocument();
    expect(screen.getByText('VS Code')).toBeInTheDocument();
    // Quick-tips empty state is NOT shown when recents are present.
    expect(screen.queryByText(/quick tips/i)).not.toBeInTheDocument();
  });

  it('does not show the "Recents" header once the user starts typing', () => {
    useAppStore.setState({
      query: 'chr',
      results: [appResult('app:chrome', 'Chrome')],
    });
    render(<ResultsList />);

    expect(screen.queryByTestId('recents-header')).not.toBeInTheDocument();
    expect(screen.getByText('Chrome')).toBeInTheDocument();
  });
});
