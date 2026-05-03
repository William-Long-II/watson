import { Scratchpad } from './Scratchpad';
import { NoteEditor } from './NoteEditor';
import { NotificationsDrawer } from './NotificationsDrawer';
import { ResultsList } from './ResultsList';
import type { PanelId } from '../stores/app';
import type { ReactNode } from 'react';

/**
 * Routes the active panel to its component, or falls through to
 * `<ResultsList />` when no panel is open. Replaces the 5-way
 * conditional ladder that used to live inline in `App.tsx` and read
 * four parallel `*Visible` booleans.
 *
 * Settings is rendered by the parent (it owns close-side state and
 * captures the `onClose` continuation for the X button), so this
 * host accepts the rendered Settings element via `settingsPanel`
 * rather than re-importing `<SettingsPanel>` itself.
 */
export function PanelHost({
  panel,
  settingsPanel,
}: {
  panel: PanelId | null;
  settingsPanel: ReactNode;
}) {
  switch (panel) {
    case 'notifications':
      return <NotificationsDrawer />;
    case 'noteEditor':
      return <NoteEditor />;
    case 'scratchpad':
      return <Scratchpad />;
    case 'settings':
      return <>{settingsPanel}</>;
    case null:
      return <ResultsList />;
  }
}
