import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { PanelHost } from '../PanelHost';
import { useAppStore } from '../../stores/app';

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
    notifications: [],
    notificationsUnread: 0,
    actionMenuOpen: false,
  });
}

describe('PanelHost — Phase 1A panel routing', () => {
  beforeEach(() => {
    resetStore();
    mockInvoke.mockReset();
    mockInvoke.mockImplementation(async () => undefined);
  });

  it('renders ResultsList when panel is null', () => {
    render(<PanelHost panel={null} settingsPanel={<div data-testid="settings-mock" />} />);
    // ResultsList renders the empty-state Quick Tips header on a clean
    // store — using that as a stable signal that the default branch
    // ran rather than depending on a list role.
    expect(screen.getByText(/quick tips/i)).toBeInTheDocument();
    expect(screen.queryByTestId('settings-mock')).toBeNull();
  });

  it('renders the supplied settings element when panel is "settings"', () => {
    render(
      <PanelHost panel="settings" settingsPanel={<div data-testid="settings-mock">settings</div>} />,
    );
    expect(screen.getByTestId('settings-mock')).toBeInTheDocument();
  });

  it('renders NotificationsDrawer when panel is "notifications"', () => {
    render(<PanelHost panel="notifications" settingsPanel={<div />} />);
    // The drawer's heading is a stable identifier.
    expect(screen.getByRole('region', { name: /notifications/i })).toBeInTheDocument();
  });

  it('renders the scratchpad when panel is "scratchpad"', () => {
    render(<PanelHost panel="scratchpad" settingsPanel={<div />} />);
    // Scratchpad uses a textarea; presence is a stable signal.
    expect(screen.getByRole('textbox')).toBeInTheDocument();
  });
});
