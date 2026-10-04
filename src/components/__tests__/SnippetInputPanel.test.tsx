import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { PanelHost } from '../PanelHost';
import { useAppStore } from '../../stores/app';
import type { SearchResult } from '../../types';

const mockInvoke = vi.mocked(invoke);

function snippetResult(expansion: string): SearchResult {
  return {
    id: 'snip:1',
    name: 'Greeting',
    description: expansion,
    result_type: 'snippet',
    score: 0,
    action: { type: 'paste_snippet', expansion },
  } as unknown as SearchResult;
}

function calls(cmd: string) {
  return mockInvoke.mock.calls.filter(([c]) => c === cmd);
}

describe('snippet input prompts', () => {
  beforeEach(() => {
    useAppStore.setState({
      query: ';hi',
      results: [],
      selectedIndex: 0,
      currentPanel: null,
      pendingSnippet: null,
    });
    mockInvoke.mockReset();
    mockInvoke.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === 'snippet_input_prompts') {
        const { expansion } = args as { expansion: string };
        return expansion.includes('{input:') ? ['Name', 'Company'] : [];
      }
      return undefined;
    });
  });

  it('pastes directly when the snippet has no inputs', async () => {
    useAppStore.setState({ results: [snippetResult('Hello {date}')] });
    await useAppStore.getState().executeSelected();

    expect(useAppStore.getState().currentPanel).toBeNull();
    expect(calls('execute_action')).toHaveLength(1);
    expect(calls('hide_window')).toHaveLength(1);
  });

  it('asks for inputs, then pastes them on Enter from the last field', async () => {
    const expansion = 'Hi {input:Name} from {input:Company}';
    useAppStore.setState({ results: [snippetResult(expansion)] });
    await useAppStore.getState().executeSelected();

    const state = useAppStore.getState();
    expect(state.currentPanel).toBe('snippetInput');
    expect(state.pendingSnippet).toEqual({ expansion, prompts: ['Name', 'Company'] });
    expect(calls('execute_action')).toHaveLength(0);
    expect(calls('hide_window')).toHaveLength(0);

    render(<PanelHost panel="snippetInput" settingsPanel={<div />} />);
    const name = screen.getByLabelText('Name');
    const company = screen.getByLabelText('Company');
    expect(name).toHaveFocus();

    fireEvent.change(name, { target: { value: 'Ada' } });
    fireEvent.keyDown(name, { key: 'Enter' });
    expect(company).toHaveFocus();
    expect(calls('paste_snippet')).toHaveLength(0);

    fireEvent.change(company, { target: { value: 'Acme' } });
    fireEvent.keyDown(company, { key: 'Enter' });
    await vi.waitFor(() => expect(calls('paste_snippet')).toHaveLength(1));

    expect(calls('paste_snippet')[0][1]).toEqual({
      expansion,
      inputs: { Name: 'Ada', Company: 'Acme' },
    });
    expect(calls('hide_window')).toHaveLength(1);
    expect(useAppStore.getState().pendingSnippet).toBeNull();
    expect(useAppStore.getState().currentPanel).toBeNull();
  });

  it('cancels without pasting on Escape', async () => {
    useAppStore.setState({ results: [snippetResult('Hi {input:Name}')] });
    await useAppStore.getState().executeSelected();

    render(<PanelHost panel="snippetInput" settingsPanel={<div />} />);
    fireEvent.keyDown(screen.getByLabelText('Name'), { key: 'Escape' });

    expect(useAppStore.getState().currentPanel).toBeNull();
    expect(useAppStore.getState().pendingSnippet).toBeNull();
    expect(calls('paste_snippet')).toHaveLength(0);
    expect(calls('execute_action')).toHaveLength(0);
  });
});
