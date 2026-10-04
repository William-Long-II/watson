import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { invoke } from '@tauri-apps/api/core';
import { ScriptCommandsSettings } from '../ScriptCommandsSettings';
import { useAppStore } from '../../stores/app';
import type { ScriptCommand, Settings } from '../../types';

const mockInvoke = vi.mocked(invoke);

function baseSettings(script_commands: ScriptCommand[] = []): Settings {
  return {
    general: { launch_at_login: false, show_in_dock: false, show_in_taskbar: false },
    activation: { hotkey: 'Alt+Space', show_tray_icon: true },
    search: { max_results: 8, show_recently_used: true, fuzzy_match_threshold: 0.6, use_frequency_ranking: true },
    theme: { mode: 'system', accent_color: 'system' },
    web_searches: [{ name: 'Google', keyword: 'g', url: 'https://google.com/?q={query}', requires_setup: false }],
    file_search: { enabled: true, indexed_paths: [], excluded_patterns: [], max_depth: 5 },
    clipboard: { ignore_patterns: [] },
    script_commands,
  };
}

function savedSettings(): Settings {
  const call = mockInvoke.mock.calls.filter(([cmd]) => cmd === 'save_settings_cmd').at(-1);
  return (call?.[1] as { settings: Settings }).settings;
}

describe('ScriptCommandsSettings — WAT-501', () => {
  beforeEach(() => {
    mockInvoke.mockReset();
    mockInvoke.mockImplementation(async () => undefined);
    useAppStore.setState({ settings: baseSettings(), reservedPrefixes: ['n', 'f', '>'] });
  });

  it('shows an empty state with no commands', () => {
    render(<ScriptCommandsSettings />);
    expect(screen.getByText(/no script commands yet/i)).toBeInTheDocument();
  });

  it('adds a script command through settings', async () => {
    const user = userEvent.setup();
    render(<ScriptCommandsSettings />);

    await user.click(screen.getByRole('button', { name: /add new/i }));
    const save = screen.getByRole('button', { name: 'Save' });
    expect(save).toBeDisabled();

    await user.type(screen.getByLabelText('Name'), 'Weather');
    await user.type(screen.getByLabelText('Keyword'), 'w');
    await user.type(screen.getByLabelText('Script path'), '~/scripts/weather.py');
    await user.type(screen.getByLabelText(/icon/i), '🌤');
    await user.click(save);

    expect(savedSettings().script_commands).toEqual([
      { name: 'Weather', keyword: 'w', script: '~/scripts/weather.py', interpreter: undefined, icon: '🌤', timeout_ms: undefined },
    ]);
    // Other settings are preserved.
    expect(savedSettings().web_searches).toHaveLength(1);
  });

  it('warns when the keyword collides with a web search', async () => {
    const user = userEvent.setup();
    render(<ScriptCommandsSettings />);
    await user.click(screen.getByRole('button', { name: /add new/i }));
    await user.type(screen.getByLabelText('Keyword'), 'g');
    expect(screen.getByRole('alert')).toHaveTextContent(/take precedence/i);
  });

  it('rejects keywords containing spaces', async () => {
    const user = userEvent.setup();
    render(<ScriptCommandsSettings />);
    await user.click(screen.getByRole('button', { name: /add new/i }));
    await user.type(screen.getByLabelText('Name'), 'X');
    await user.type(screen.getByLabelText('Keyword'), 'a b');
    await user.type(screen.getByLabelText('Script path'), '/x.sh');
    expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled();
  });

  it('edits and deletes an existing command', async () => {
    const user = userEvent.setup();
    useAppStore.setState({
      settings: baseSettings([{ name: 'Weather', keyword: 'w', script: '/w.py', timeout_ms: 2000 }]),
    });
    render(<ScriptCommandsSettings />);

    await user.click(screen.getByText('Weather'));
    const nameInput = screen.getByLabelText('Name');
    await user.clear(nameInput);
    await user.type(nameInput, 'Forecast');
    await user.click(screen.getByRole('button', { name: 'Save' }));
    expect(savedSettings().script_commands[0]).toMatchObject({ name: 'Forecast', timeout_ms: 2000 });

    await user.click(screen.getByText('Forecast'));
    await user.click(screen.getByRole('button', { name: 'Delete' }));
    // The confirm modal's Delete is the last one rendered.
    await user.click(screen.getAllByRole('button', { name: 'Delete' }).at(-1)!);
    expect(savedSettings().script_commands).toEqual([]);
  });
});
