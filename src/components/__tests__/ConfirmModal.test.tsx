import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { ConfirmModal } from '../ConfirmModal';

function noop() {}

describe('ConfirmModal (WAT-405)', () => {
  it('renders nothing when open=false', () => {
    render(
      <ConfirmModal
        open={false}
        title="t"
        message="m"
        onConfirm={noop}
        onCancel={noop}
      />,
    );
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });

  it('renders a labelled dialog when open=true', () => {
    render(
      <ConfirmModal
        open
        title="Delete this note?"
        message="This can't be undone."
        onConfirm={noop}
        onCancel={noop}
      />,
    );
    const dialog = screen.getByRole('dialog');
    expect(dialog).toHaveAttribute('aria-modal', 'true');
    expect(dialog).toHaveAttribute('aria-labelledby', 'confirm-modal-title');
    expect(screen.getByText('Delete this note?')).toBeInTheDocument();
    expect(screen.getByText(/can't be undone/i)).toBeInTheDocument();
  });

  it('focuses the confirm button on open', () => {
    render(
      <ConfirmModal
        open
        title="t"
        message="m"
        confirmLabel="OK"
        onConfirm={noop}
        onCancel={noop}
      />,
    );
    expect(screen.getByRole('button', { name: 'OK' })).toHaveFocus();
  });

  it('Enter fires onConfirm', async () => {
    const onConfirm = vi.fn();
    const user = userEvent.setup();
    render(
      <ConfirmModal
        open
        title="t"
        message="m"
        onConfirm={onConfirm}
        onCancel={noop}
      />,
    );
    await user.keyboard('{Enter}');
    expect(onConfirm).toHaveBeenCalledTimes(1);
  });

  it('Escape fires onCancel', async () => {
    const onCancel = vi.fn();
    const user = userEvent.setup();
    render(
      <ConfirmModal
        open
        title="t"
        message="m"
        onConfirm={noop}
        onCancel={onCancel}
      />,
    );
    await user.keyboard('{Escape}');
    expect(onCancel).toHaveBeenCalledTimes(1);
  });

  it('clicking the backdrop cancels; clicking the panel does not', async () => {
    const onCancel = vi.fn();
    const user = userEvent.setup();
    render(
      <ConfirmModal
        open
        title="t"
        message="m"
        onConfirm={noop}
        onCancel={onCancel}
      />,
    );
    const dialog = screen.getByRole('dialog');
    // Click directly on the dialog (overlay backdrop) — e.target === e.currentTarget.
    await user.click(dialog);
    expect(onCancel).toHaveBeenCalledTimes(1);

    // Clicking inside the inner panel (title) should NOT trigger cancel.
    onCancel.mockReset();
    await user.click(screen.getByText('t'));
    expect(onCancel).not.toHaveBeenCalled();
  });

  it('danger variant applies red styling to the confirm button', () => {
    render(
      <ConfirmModal
        open
        title="t"
        message="m"
        confirmLabel="Delete"
        variant="danger"
        onConfirm={noop}
        onCancel={noop}
      />,
    );
    const btn = screen.getByRole('button', { name: 'Delete' });
    expect(btn.className).toMatch(/bg-red/);
  });

  it('Tab cycles focus between cancel and confirm (focus trap)', async () => {
    const user = userEvent.setup();
    render(
      <ConfirmModal
        open
        title="t"
        message="m"
        cancelLabel="No"
        confirmLabel="Yes"
        onConfirm={noop}
        onCancel={noop}
      />,
    );
    // Starts on confirm (Yes). Tab → cycles to cancel (No).
    expect(screen.getByRole('button', { name: 'Yes' })).toHaveFocus();
    await user.tab();
    expect(screen.getByRole('button', { name: 'No' })).toHaveFocus();
    // Tab again from the last focusable — trap cycles back to first.
    await user.tab();
    expect(screen.getByRole('button', { name: 'Yes' })).toHaveFocus();
  });
});
