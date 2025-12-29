export interface SearchResult {
  id: string;
  name: string;
  description: string;
  icon: string | null;
  result_type: 'application' | 'web_search' | 'system_command' | 'clipboard';
  score: number;
  action: SearchAction;
}

export type SearchAction =
  | { type: 'launch_app'; path: string }
  | { type: 'open_url'; url: string }
  | { type: 'run_command'; command: string }
  | { type: 'copy_clipboard'; content: string };

export interface Settings {
  general: GeneralSettings;
  activation: ActivationSettings;
  search: SearchSettings;
  theme: ThemeSettings;
  web_searches: WebSearch[];
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
