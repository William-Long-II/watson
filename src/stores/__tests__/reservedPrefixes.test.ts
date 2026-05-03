import { describe, it, expect, beforeEach, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { useAppStore } from '../app';

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

describe('store.loadReservedPrefixes (WAT-205)', () => {
  beforeEach(() => {
    resetStore();
    mockInvoke.mockReset();
  });

  it('is empty before load so the UI can render without a populated list', () => {
    expect(useAppStore.getState().reservedPrefixes).toEqual([]);
  });

  it('populates reservedPrefixes from the backend get_reserved_prefixes command', async () => {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'get_reserved_prefixes') return ['n', 'notes', 'f', 'files', 'cb', 'clip', 'cap', 'captures', 's', '>'];
      return undefined;
    });

    await useAppStore.getState().loadReservedPrefixes();

    const state = useAppStore.getState();
    expect(state.reservedPrefixes).toContain('n');
    expect(state.reservedPrefixes).toContain('>');
    expect(state.reservedPrefixes).toContain('s');
    expect(state.reservedPrefixes).toContain('cap');
    // Length check is a sanity guard — if the Rust side adds a prefix, the
    // Vitest mock here needs updating too, and this assertion will nudge
    // the person making the change.
    expect(state.reservedPrefixes).toHaveLength(10);
  });

  it('leaves reservedPrefixes untouched when the backend call fails', async () => {
    mockInvoke.mockRejectedValue(new Error('ipc failed'));
    // Seed a known-empty starting state so we can assert no mutation.
    useAppStore.setState({ reservedPrefixes: [] });

    await useAppStore.getState().loadReservedPrefixes();

    expect(useAppStore.getState().reservedPrefixes).toEqual([]);
  });
});
