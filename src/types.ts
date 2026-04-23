export interface SearchResult {
  id: string;
  name: string;
  description: string;
  icon: string | null;
  result_type: 'application' | 'web_search' | 'system_command' | 'clipboard' | 'note' | 'file' | 'calculation' | 'snippet';
  score: number;
  /** WAT-303: populated only for clipboard results; true when pinned. */
  pinned?: boolean;
  action: SearchAction;
}

export type SearchAction =
  | { type: 'launch_app'; path: string }
  | { type: 'open_url'; url: string }
  | { type: 'run_command'; command: string }
  | { type: 'copy_clipboard'; content: string }
  | { type: 'open_note'; note_id: string }
  | { type: 'open_file'; path: string }
  | { type: 'paste_snippet'; expansion: string };

export interface Snippet {
  id: string;
  trigger: string;
  name: string;
  expansion: string;
  created_at: number;
  modified_at: number;
}

export interface Settings {
  general: GeneralSettings;
  activation: ActivationSettings;
  search: SearchSettings;
  theme: ThemeSettings;
  web_searches: WebSearch[];
  file_search: FileSearchSettings;
  clipboard: ClipboardSettings;
}

export interface ClipboardSettings {
  /** WAT-303: regex patterns; clipboard entries matching any are filtered before being recorded. */
  ignore_patterns: string[];
}

export interface GeneralSettings {
  launch_at_login: boolean;
  show_in_dock: boolean;
  show_in_taskbar: boolean;
}

export interface ActivationSettings {
  hotkey: string;
  show_tray_icon: boolean;
}

export interface SearchSettings {
  max_results: number;
  show_recently_used: boolean;
  fuzzy_match_threshold: number;
  use_frequency_ranking: boolean;
}

export interface ThemeSettings {
  mode: 'light' | 'dark' | 'system';
  accent_color: string;
  custom?: CustomTheme;
}

export interface CustomTheme {
  background?: string;
  foreground?: string;
  border?: string;
  selected_background?: string;
  input_background?: string;
  font_family?: string;
  font_size?: number;
  border_radius?: number;
}

export interface WebSearch {
  name: string;
  keyword: string;
  url: string;
  icon?: string;
  requires_setup: boolean;
  instance?: string;
}

export interface Scratchpad {
  content: string;
  modified_at: number;
}

export interface Note {
  id: string;
  title: string;
  content: string;
  tags: string[];
  created_at: number;
  modified_at: number;
  /**
   * WAT-204: present only when `get_note` detected that the on-disk .md
   * file was modified outside Watson since the DB was last updated.
   * The editor renders a reconcile banner offering "Use disk" / "Keep DB".
   */
  external_changes?: ExternalChanges;
}

export interface ExternalChanges {
  disk_title: string;
  disk_content: string;
  disk_modified_at: number;
}

export interface FileEntry {
  id: string;
  name: string;
  path: string;
  extension: string | null;
  size_bytes: number | null;
  modified_at: number;
}

export interface FileSearchSettings {
  enabled: boolean;
  indexed_paths: string[];
  excluded_patterns: string[];
  max_depth: number;
}

export type StartupWarningKind =
  | 'shortcut_unavailable'
  | 'settings_from_newer_version'
  | 'invalid_clipboard_filter';

// WAT-406: notifications drawer

export type NotificationSeverity = 'info' | 'warning' | 'error';

export interface Notification {
  id: string;
  severity: NotificationSeverity;
  title: string;
  message: string;
  created_at: number;
  dismissed?: boolean;
}

export interface StartupWarning {
  id: string;
  kind: StartupWarningKind;
  message: string;
}
