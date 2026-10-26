import { invoke, isTauri } from '@tauri-apps/api/core';
import type { AudioSource, ConnectionReport, Language, Settings, SessionStatus } from './types';
import type { MenuModel, ShortcutStatus, SaveResult } from './lib/desktop';
export const desktop = isTauri();
export const api = {
  syncDesktop: (model: MenuModel) => invoke<void>('sync_desktop', { model }),
  shortcutStatus: () => invoke<ShortcutStatus>('shortcut_status'),
  suspendShortcut: (suspended: boolean) => invoke<void>('suspend_shortcut', { suspended }),
  show: () => invoke<void>('show_main_window'),
  load: () => invoke<Settings>('load_settings'),
  save: (settings: Settings) => invoke<SaveResult>('save_settings', { settings }),
  directory: () => invoke<string>('config_directory'),
  openDirectory: () => invoke<void>('open_config_directory'),
  languages: () => invoke<Language[]>('supported_languages'),
  sources: () => invoke<AudioSource[]>('list_sources'),
  start: (sourceId: string) => invoke<number>('start_translation', { sourceId }),
  stop: () => invoke<void>('stop_translation'),
  status: () => invoke<SessionStatus>('session_status'),
  volumes: (original: number, translation: number) => invoke<void>('set_volumes', { original, translation }),
  checkConnection: () => invoke<ConnectionReport>('check_connection'),
  ready: () => invoke<void>('frontend_ready'),
  beginClose: () => invoke<void>('begin_close'),
  cancelClose: () => invoke<void>('cancel_close'),
  finishClose: () => invoke<void>('finish_close'),
};
