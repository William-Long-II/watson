import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { invoke } from '@tauri-apps/api/core';
import { StartupWarningBanner } from '../StartupWarningBanner';
import { useAppStore } from '../../stores/app';
import type { StartupWarning } from '../../types';

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
  });
}

function shortcutWarning(overrides: Partial<StartupWarning> = {}): StartupWarning {
  return {
    id: overrides.id ?? 'shortcut-unavailable:Alt+Space',
    kind: overrides.kind ?? 'shortcut_unavailable',
    message:
      overrides.message ??
      "Couldn't register hotkey Alt+Space: already in use. Pick a different hotkey in Settings.",
  };
}

describe('StartupWarningBanner', () => {
  beforeEach(() => {
    resetStore();
    mockInvoke.mockReset();
    // Default invoke: no warnings. Individual tests override.
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'get_startup_warnings') return [] as StartupWarning[];
      return undefined;
    });
  });

  it('renders nothing when there are no warnings', async () => {
    const { container } = render(<StartupWarningBanner onOpenSettings={() => {}} />);
    // Let the effect's invoke settle.
    await vi.waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith('get_startup_warnings');
    });
    expect(container.firstChild).toBeNull();
  });

  it('renders the warning message when the backend returns one', async () => {
    const warning = shortcutWarning();
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'get_startup_warnings') return [warning];
      return undefined;
    });

    render(<StartupWarningBanner onOpenSettings={() => {}} />);

    expect(await screen.findByText(/couldn't register hotkey alt\+space/i)).toBeInTheDocument();
  });

  it('shows a "Change hotkey" action for shortcut_unavailable warnings', async () => {
    const onOpenSettings = vi.fn();
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'get_startup_warnings') return [shortcutWarning()];
      return undefined;
    });

    const user = userEvent.setup();
    render(<StartupWarningBanner onOpenSettings={onOpenSettings} />);

    const button = await screen.findByRole('button', { name: /change hotkey/i });
    await user.click(button);

    expect(onOpenSettings).toHaveBeenCalledTimes(1);
  });

  it('dismiss button invokes dismiss_startup_warning and removes the banner', async () => {
    const warning = shortcutWarning({ id: 'shortcut-unavailable:Ctrl+Space' });
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'get_startup_warnings') return [warning];
      if (cmd === 'dismiss_startup_warning') return true;
      return undefined;
    });

    const user = userEvent.setup();
    render(<StartupWarningBanner onOpenSettings={() => {}} />);

    const dismiss = await screen.findByRole('button', {
      name: /dismiss warning:/i,
    });
    await user.click(dismiss);

    expect(mockInvoke).toHaveBeenCalledWith('dismiss_startup_warning', {
      id: 'shortcut-unavailable:Ctrl+Space',
    });

    // Optimistic local removal — the warning is gone from the DOM before
    // the store observes the backend response.
    expect(screen.queryByText(warning.message)).not.toBeInTheDocument();
  });

  it("renders multiple warnings side-by-side (one per row) — each with its own dismiss", async () => {
    const warnings: StartupWarning[] = [
      shortcutWarning({ id: 'a', message: 'first' }),
      shortcutWarning({ id: 'b', message: 'second' }),
    ];
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'get_startup_warnings') return warnings;
      return undefined;
    });

    render(<StartupWarningBanner onOpenSettings={() => {}} />);

    expect(await screen.findByText('first')).toBeInTheDocument();
    expect(await screen.findByText('second')).toBeInTheDocument();
    const dismissButtons = await screen.findAllByRole('button', {
      name: /dismiss warning:/i,
    });
    expect(dismissButtons).toHaveLength(2);
  });

  it('uses role="alert" with aria-live so screen readers announce new warnings', async () => {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'get_startup_warnings') return [shortcutWarning()];
      return undefined;
    });

    render(<StartupWarningBanner onOpenSettings={() => {}} />);

    const alert = await screen.findByRole('alert');
    expect(alert).toHaveAttribute('aria-live', 'polite');
  });

  // --- WAT-206 ---

  it('shows "Open config folder" button for settings_from_newer_version kind', async () => {
    const warning: StartupWarning = {
      id: 'settings-from-newer-version',
      kind: 'settings_from_newer_version',
      message: 'Your config was written by a newer Watson.',
    };
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'get_startup_warnings') return [warning];
      if (cmd === 'open_config_folder') return undefined;
      return undefined;
    });

    const user = userEvent.setup();
    render(<StartupWarningBanner onOpenSettings={() => {}} />);

    const button = await screen.findByRole('button', { name: /open config folder/i });
    await user.click(button);

    expect(mockInvoke).toHaveBeenCalledWith('open_config_folder');
  });

  it('does not show "Change hotkey" for settings_from_newer_version kind', async () => {
    const warning: StartupWarning = {
      id: 'settings-from-newer-version',
      kind: 'settings_from_newer_version',
      message: 'Your config was written by a newer Watson.',
    };
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'get_startup_warnings') return [warning];
      return undefined;
    });

    render(<StartupWarningBanner onOpenSettings={() => {}} />);

    await screen.findByText(warning.message);
    expect(screen.queryByRole('button', { name: /change hotkey/i })).not.toBeInTheDocument();
  });
});
