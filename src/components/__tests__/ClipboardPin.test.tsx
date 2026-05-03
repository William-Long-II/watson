import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { invoke } from '@tauri-apps/api/core';
import { ResultItem } from '../ResultItem';
import { useAppStore } from '../../stores/app';
import type { SearchResult } from '../../types';

const mockInvoke = vi.mocked(invoke);

function resetStore() {
  useAppStore.setState({
    query: 'cb',
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

function clip(overrides: Partial<SearchResult> = {}): SearchResult {
  return {
    id: overrides.id ?? 'clip:abc',
    name: overrides.name ?? 'hello world',
    description: overrides.description ?? 'Copied 12:34:56',
    icon: null,
    result_type: 'clipboard',
    score: 10000,
    pinned: overrides.pinned,
    action: { type: 'copy_clipboard', content: 'hello world' },
  } as SearchResult;
}

describe('ResultItem — WAT-303 pin toggle', () => {
  beforeEach(() => {
    resetStore();
    mockInvoke.mockReset();
    // `pinClipboardEntry` re-runs `setQuery(query)` which invokes
    // `search` — that command must return an array or the store's
    // resizeWindow() trips on undefined.length.
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'search') return [] as SearchResult[];
      return undefined;
    });
  });

  it('renders a pin button only for clipboard results', () => {
    const { rerender } = render(
      <ResultItem result={clip()} isSelected={false} onClick={() => {}} index={0} />,
    );
    expect(screen.getByRole('button', { name: /pin clipboard entry/i })).toBeInTheDocument();

    // Swap in a non-clipboard result — pin button should disappear.
    rerender(
      <ResultItem
        result={{ ...clip(), result_type: 'application' }}
        isSelected={false}
        onClick={() => {}}
        index={0}
      />,
    );
    expect(screen.queryByRole('button', { name: /pin clipboard entry/i })).not.toBeInTheDocument();
  });

  it('shows "Unpin" state and aria-pressed=true when already pinned', () => {
    render(
      <ResultItem
        result={clip({ pinned: true })}
        isSelected={false}
        onClick={() => {}}
        index={0}
      />,
    );
    const btn = screen.getByRole('button', { name: /unpin clipboard entry/i });
    expect(btn).toHaveAttribute('aria-pressed', 'true');
  });

  it('clicking an unpinned entry fires pin_clipboard_entry', async () => {
    useAppStore.setState({ results: [clip({ id: 'clip:1' })] });
    const user = userEvent.setup();
    render(
      <ResultItem
        result={clip({ id: 'clip:1' })}
        isSelected={false}
        onClick={() => {}}
        index={0}
      />,
    );

    await user.click(screen.getByRole('button', { name: /pin clipboard entry/i }));

    expect(mockInvoke).toHaveBeenCalledWith('pin_clipboard_entry', { id: 'clip:1' });
  });

  it('clicking a pinned entry fires unpin_clipboard_entry', async () => {
    useAppStore.setState({ results: [clip({ id: 'clip:2', pinned: true })] });
    const user = userEvent.setup();
    render(
      <ResultItem
        result={clip({ id: 'clip:2', pinned: true })}
        isSelected={false}
        onClick={() => {}}
        index={0}
      />,
    );

    await user.click(screen.getByRole('button', { name: /unpin clipboard entry/i }));

    expect(mockInvoke).toHaveBeenCalledWith('unpin_clipboard_entry', { id: 'clip:2' });
  });

  it('clicking the pin button does NOT fire the row-level onClick', async () => {
    // The row onClick is the "execute action" path (copy-to-clipboard for
    // clipboard results). Clicking the pin toggle must not accidentally
    // trigger a copy too.
    const rowClick = vi.fn();
    useAppStore.setState({ results: [clip({ id: 'clip:3' })] });
    const user = userEvent.setup();
    render(<ResultItem result={clip({ id: 'clip:3' })} isSelected={false} onClick={rowClick} index={0} />);

    await user.click(screen.getByRole('button', { name: /pin clipboard entry/i }));

    expect(rowClick).not.toHaveBeenCalled();
  });
});
