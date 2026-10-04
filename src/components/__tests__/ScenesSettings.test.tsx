import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { invoke } from '@tauri-apps/api/core';
import { ScenesSettings, isSceneInputValid } from '../ScenesSettings';
import type { Scene } from '../../types';

const mockInvoke = vi.mocked(invoke);

function scene(overrides: Partial<Scene> = {}): Scene {
  return {
    id: 'scene:1',
    name: 'Start Work',
    icon: null,
    steps: [
      { type: 'launch_app', path: '/Applications/Slack.app' },
      { type: 'open_url', url: 'https://calendar.google.com' },
    ],
    inter_step_delay_ms: 200,
    created_at: 1_700_000_000,
    modified_at: 1_700_000_000,
    ...overrides,
  };
}

describe('ScenesSettings — Phase 2a CRUD', () => {
  beforeEach(() => {
    mockInvoke.mockReset();
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'list_scenes') return [] as Scene[];
      return undefined;
    });
  });

  it('shows an empty state when no scenes exist', async () => {
    render(<ScenesSettings />);
    expect(await screen.findByText(/no scenes yet/i)).toBeInTheDocument();
    expect(mockInvoke).toHaveBeenCalledWith('list_scenes');
  });

  it('creates a scene with apps and URLs in order', async () => {
    const user = userEvent.setup();
    render(<ScenesSettings />);
    await screen.findByText(/no scenes yet/i);
    await user.click(screen.getByRole('button', { name: /add scene/i }));

    await user.type(screen.getByLabelText('Name'), 'Start Work');
    await user.type(screen.getByLabelText('Step 1 app path'), '/Applications/Slack.app');

    await user.click(screen.getByRole('button', { name: /add step/i }));
    await user.selectOptions(screen.getByLabelText('Step 2 type'), 'open_url');
    await user.type(screen.getByLabelText('Step 2 URL'), 'https://calendar.google.com');

    await user.click(screen.getByRole('button', { name: /^save$/i }));

    const call = mockInvoke.mock.calls.find((c) => c[0] === 'create_scene');
    expect(call?.[1]).toEqual({
      scene: {
        name: 'Start Work',
        icon: null,
        inter_step_delay_ms: 200,
        steps: [
          { type: 'launch_app', path: '/Applications/Slack.app' },
          { type: 'open_url', url: 'https://calendar.google.com' },
        ],
      },
    });
  });

  it('Save stays disabled until the name and every step target are filled', async () => {
    const user = userEvent.setup();
    render(<ScenesSettings />);
    await screen.findByText(/no scenes yet/i);
    await user.click(screen.getByRole('button', { name: /add scene/i }));

    const save = screen.getByRole('button', { name: /^save$/i });
    expect(save).toBeDisabled();
    await user.type(screen.getByLabelText('Name'), 'X');
    expect(save).toBeDisabled();
    await user.type(screen.getByLabelText('Step 1 app path'), '/a.app');
    expect(save).toBeEnabled();
  });

  it('editing reorders steps and saves through update_scene', async () => {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'list_scenes') return [scene()];
      return undefined;
    });
    const user = userEvent.setup();
    render(<ScenesSettings />);

    await user.click(await screen.findByRole('button', { name: /edit scene start work/i }));
    expect(screen.getByDisplayValue('Start Work')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Move step 2 up' }));
    await user.click(screen.getByRole('button', { name: /^save$/i }));

    const call = mockInvoke.mock.calls.find((c) => c[0] === 'update_scene');
    expect(call?.[1]).toMatchObject({
      id: 'scene:1',
      scene: {
        steps: [
          { type: 'open_url', url: 'https://calendar.google.com' },
          { type: 'launch_app', path: '/Applications/Slack.app' },
        ],
      },
    });
  });

  it('Run fires execute_action with run_scene', async () => {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'list_scenes') return [scene({ id: 'scene:42' })];
      return undefined;
    });
    const user = userEvent.setup();
    render(<ScenesSettings />);

    await user.click(await screen.findByRole('button', { name: /run scene start work/i }));
    expect(mockInvoke).toHaveBeenCalledWith('execute_action', {
      action: { type: 'run_scene', scene_id: 'scene:42' },
    });
  });

  it('delete only fires after confirming in the modal', async () => {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'list_scenes') return [scene({ id: 'scene:7' })];
      return undefined;
    });
    const user = userEvent.setup();
    render(<ScenesSettings />);

    await user.click(await screen.findByRole('button', { name: /edit scene start work/i }));
    await user.click(screen.getByRole('button', { name: /delete/i }));
    const dialog = screen.getByRole('dialog');
    expect(mockInvoke.mock.calls.find((c) => c[0] === 'delete_scene')).toBeUndefined();

    const confirm = screen.getAllByRole('button', { name: /delete/i }).find((b) => dialog.contains(b));
    await user.click(confirm!);
    expect(mockInvoke).toHaveBeenCalledWith('delete_scene', { id: 'scene:7' });
  });

  it('shows a backend validation error under the editor', async () => {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'list_scenes') return [];
      if (cmd === 'create_scene') throw 'step 1: step is missing its app path';
      return undefined;
    });
    const user = userEvent.setup();
    render(<ScenesSettings />);
    await screen.findByText(/no scenes yet/i);
    await user.click(screen.getByRole('button', { name: /add scene/i }));
    await user.type(screen.getByLabelText('Name'), 'X');
    await user.type(screen.getByLabelText('Step 1 app path'), 'y');
    await user.click(screen.getByRole('button', { name: /^save$/i }));
    expect(await screen.findByRole('alert')).toHaveTextContent(/missing its app path/);
  });
});

describe('isSceneInputValid', () => {
  it('requires a focus_window step to name its app but not its title', () => {
    const base = { name: 'x', icon: null, inter_step_delay_ms: 0 };
    expect(isSceneInputValid({ ...base, steps: [{ type: 'focus_window', app: 'Code', title: '' }] })).toBe(true);
    expect(isSceneInputValid({ ...base, steps: [{ type: 'focus_window', app: ' ', title: 'x' }] })).toBe(false);
    expect(isSceneInputValid({ ...base, steps: [] })).toBe(false);
  });
});
