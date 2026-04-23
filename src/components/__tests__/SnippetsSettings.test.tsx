import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { invoke } from '@tauri-apps/api/core';
import { SnippetsSettings } from '../SnippetsSettings';
import type { Snippet } from '../../types';

const mockInvoke = vi.mocked(invoke);

function snippet(overrides: Partial<Snippet> = {}): Snippet {
  return {
    id: overrides.id ?? 'snip:1',
    trigger: overrides.trigger ?? ';addr',
    name: overrides.name ?? 'Home Address',
    expansion: overrides.expansion ?? '123 Main St',
    created_at: overrides.created_at ?? 1_700_000_000,
    modified_at: overrides.modified_at ?? 1_700_000_000,
  };
}

describe('SnippetsSettings — WAT-301 CRUD', () => {
  beforeEach(() => {
    mockInvoke.mockReset();
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'list_snippets') return [] as Snippet[];
      return undefined;
    });
  });

  it('shows an empty-state when no snippets exist', async () => {
    render(<SnippetsSettings />);
    // list_snippets is invoked on mount.
    await vi.waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith('list_snippets');
    });
    expect(await screen.findByText(/no snippets yet/i)).toBeInTheDocument();
  });

  it('renders existing snippets in the list', async () => {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'list_snippets') return [snippet({ id: 'snip:1', trigger: ';addr', name: 'Home' })];
      return undefined;
    });

    render(<SnippetsSettings />);

    expect(await screen.findByText(';addr')).toBeInTheDocument();
    expect(screen.getByText('Home')).toBeInTheDocument();
  });

  it('clicking "Add New" shows an editor and creating invokes create_snippet', async () => {
    const user = userEvent.setup();
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'list_snippets') return [] as Snippet[];
      if (cmd === 'create_snippet') return snippet();
      return undefined;
    });
    render(<SnippetsSettings />);

    await screen.findByText(/no snippets yet/i);
    await user.click(screen.getByRole('button', { name: /add new/i }));

    // Fill the form.
    await user.type(screen.getByPlaceholderText(';addr'), ';hello');
    await user.type(screen.getByPlaceholderText('Home Address'), 'Hello');
    await user.type(screen.getByPlaceholderText(/pasted/i), 'Hi there');

    await user.click(screen.getByRole('button', { name: /^save$/i }));

    const createCall = mockInvoke.mock.calls.find((c) => c[0] === 'create_snippet');
    expect(createCall, 'create_snippet should be invoked').toBeDefined();
    expect(createCall?.[1]).toEqual({
      trigger: ';hello',
      name: 'Hello',
      expansion: 'Hi there',
    });
  });

  it('Save is disabled when required fields are empty', async () => {
    const user = userEvent.setup();
    render(<SnippetsSettings />);
    await screen.findByText(/no snippets yet/i);
    await user.click(screen.getByRole('button', { name: /add new/i }));

    const save = screen.getByRole('button', { name: /^save$/i });
    expect(save).toBeDisabled();

    // Fill only one field — still disabled.
    await user.type(screen.getByPlaceholderText(';addr'), ';x');
    expect(save).toBeDisabled();
  });

  it('clicking an existing row opens the editor with its values prefilled', async () => {
    const existing = snippet({ id: 'snip:1', trigger: ';sig', name: 'Signature', expansion: '--\nWill' });
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'list_snippets') return [existing];
      return undefined;
    });
    const user = userEvent.setup();
    render(<SnippetsSettings />);

    await screen.findByText(';sig');
    await user.click(screen.getByText(';sig'));

    // Inputs prefilled.
    expect(screen.getByDisplayValue(';sig')).toBeInTheDocument();
    expect(screen.getByDisplayValue('Signature')).toBeInTheDocument();
    expect(screen.getByDisplayValue(/Will/)).toBeInTheDocument();
    // Delete button is visible only when editing an existing snippet.
    expect(screen.getByRole('button', { name: /delete/i })).toBeInTheDocument();
  });

  it('delete invokes delete_snippet with the id', async () => {
    const existing = snippet({ id: 'snip:42' });
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'list_snippets') return [existing];
      return undefined;
    });
    const user = userEvent.setup();
    render(<SnippetsSettings />);

    await screen.findByText(';addr');
    await user.click(screen.getByText(';addr'));
    await user.click(screen.getByRole('button', { name: /delete/i }));

    expect(mockInvoke).toHaveBeenCalledWith('delete_snippet', { id: 'snip:42' });
  });
});
