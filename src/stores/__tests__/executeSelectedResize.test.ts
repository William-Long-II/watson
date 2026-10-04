import { describe, it, expect, beforeEach, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { useAppStore } from '../app';
import type { SearchResult } from '../../types';

const mockInvoke = vi.mocked(invoke);

function makeResult(i: number, action: SearchResult['action']): SearchResult {
  return {
    id: `r${i}`,
    name: `Result ${i}`,
    description: '',
    icon: null,
    result_type: 'application',
    score: 100,
    usage_bonus: 0,
    action,
  } as SearchResult;
}

// Regression guard for the "reopened window is over-tall" bug: launching
// a result must shrink the window back to the empty-state height BEFORE
// hiding it, otherwise the next Alt+Space shows Quick Tips with a tall
// blank band sized to the previous result list.
describe('store.executeSelected resizes before hiding', () => {
  let calls: { cmd: string; args?: unknown }[];

  beforeEach(() => {
    calls = [];
    mockInvoke.mockReset();
    mockInvoke.mockImplementation(async (cmd: string, args?: unknown) => {
      calls.push({ cmd, args });
      return undefined;
    });
    useAppStore.setState({ query: 'fir', selectedIndex: 0, currentPanel: null });
  });

  it.each([
    ['launch_app', { type: 'launch_app', path: '/bin/firefox' }],
    ['focus_window', { type: 'focus_window', hwnd: 1 }],
  ])('%s: resize_window to the empty-state height, then hide_window', async (_name, action) => {
    const results = Array.from({ length: 5 }, (_, i) =>
      makeResult(i, action as SearchResult['action']),
    );
    useAppStore.setState({ results });

    await useAppStore.getState().executeSelected();

    const cmds = calls.map((c) => c.cmd);
    const resizeIdx = cmds.indexOf('resize_window');
    const hideIdx = cmds.indexOf('hide_window');
    expect(resizeIdx).toBeGreaterThanOrEqual(0);
    expect(hideIdx).toBeGreaterThan(resizeIdx);

    // Height must be the empty-state size, not the 5-row results size.
    const { height } = calls[resizeIdx].args as { height: number };
    const fiveRowHeight = 56 + 56 + 5 * 64 + 28;
    expect(height).toBeLessThan(fiveRowHeight);
    expect(useAppStore.getState().results).toEqual([]);
  });
});
