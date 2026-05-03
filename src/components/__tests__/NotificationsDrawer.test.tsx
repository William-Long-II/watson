import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { invoke } from '@tauri-apps/api/core';
import { NotificationsDrawer } from '../NotificationsDrawer';
import { useAppStore } from '../../stores/app';
import type { Notification } from '../../types';

const mockInvoke = vi.mocked(invoke);

function resetStore() {
  useAppStore.setState({
    query: '',
    results: [],
    selectedIndex: 0,
    settings: null,
    isLoading: false,
    currentPanel: 'notifications', // test drawer in its open state
    scratchpad: '',
    currentNote: null,
    startupWarnings: [],
    reservedPrefixes: [],
    notifications: [],
    notificationsUnread: 0,
  });
}

function notif(overrides: Partial<Notification> = {}): Notification {
  return {
    id: overrides.id ?? 'notif:1',
    severity: overrides.severity ?? 'warning',
    title: overrides.title ?? 'Hotkey unavailable',
    message: overrides.message ?? "Couldn't register Alt+Space: already in use.",
    created_at: overrides.created_at ?? 1_750_000_000,
    dismissed: overrides.dismissed,
  };
}

describe('NotificationsDrawer — WAT-406', () => {
  beforeEach(() => {
    resetStore();
    mockInvoke.mockReset();
    mockInvoke.mockImplementation(async () => undefined);
  });

  // Gating moved to <PanelHost> — NotificationsDrawer is only rendered
  // when currentPanel === 'notifications'. The host-level routing is
  // covered in PanelHost.test.tsx, so this file no longer asserts the
  // self-gating behavior the drawer used to do internally.

  it('shows an empty-state message when there are no notifications', () => {
    render(<NotificationsDrawer />);
    expect(screen.getByText(/no notifications/i)).toBeInTheDocument();
  });

  it('renders active notifications with title + message', () => {
    useAppStore.setState({
      notifications: [
        notif({ id: 'a', title: 'Hotkey unavailable', message: 'Alt+Space taken' }),
        notif({ id: 'b', title: 'Filter error', message: 'invalid regex' }),
      ],
      notificationsUnread: 2,
    });
    render(<NotificationsDrawer />);

    expect(screen.getByText('Hotkey unavailable')).toBeInTheDocument();
    expect(screen.getByText('Alt+Space taken')).toBeInTheDocument();
    expect(screen.getByText('Filter error')).toBeInTheDocument();
  });

  it('clicking the per-row X dismisses the notification via IPC', async () => {
    useAppStore.setState({
      notifications: [notif({ id: 'notif:42' })],
      notificationsUnread: 1,
    });
    const user = userEvent.setup();
    render(<NotificationsDrawer />);

    await user.click(screen.getByRole('button', { name: /dismiss: hotkey unavailable/i }));

    expect(mockInvoke).toHaveBeenCalledWith('dismiss_notification', { id: 'notif:42' });
    // Optimistic update: row moved to dismissed group; unread count now 0.
    expect(useAppStore.getState().notificationsUnread).toBe(0);
  });

  it('"Dismiss all" invokes dismiss_all_notifications and flips all to dismissed', async () => {
    useAppStore.setState({
      notifications: [
        notif({ id: 'a' }),
        notif({ id: 'b' }),
      ],
      notificationsUnread: 2,
    });
    const user = userEvent.setup();
    render(<NotificationsDrawer />);

    await user.click(screen.getByRole('button', { name: /dismiss all/i }));

    expect(mockInvoke).toHaveBeenCalledWith('dismiss_all_notifications');
    expect(useAppStore.getState().notificationsUnread).toBe(0);
  });

  it('separates active from dismissed with a heading, and dismissed rows have no X', () => {
    useAppStore.setState({
      notifications: [
        notif({ id: 'a', title: 'Active one' }),
        notif({ id: 'b', title: 'Gone one', dismissed: true }),
      ],
      notificationsUnread: 1,
    });
    render(<NotificationsDrawer />);

    expect(screen.getByText('Active one')).toBeInTheDocument();
    expect(screen.getByText('Gone one')).toBeInTheDocument();
    // Only the active row has a dismiss button; dismissed ones don't.
    expect(screen.getAllByRole('button', { name: /dismiss:/i })).toHaveLength(1);
    // Heading appears above dismissed entries.
    expect(screen.getByText(/dismissed/i)).toBeInTheDocument();
  });

  it('close button clears the active panel', async () => {
    useAppStore.setState({ notifications: [notif()] });
    const user = userEvent.setup();
    render(<NotificationsDrawer />);

    await user.click(screen.getByRole('button', { name: /close notifications/i }));

    expect(useAppStore.getState().currentPanel).toBeNull();
  });
});
