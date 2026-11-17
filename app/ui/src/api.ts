// Typed wrappers around the shell's commands and events (app/src-tauri/src/main.rs).
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

export type MediaStatus = 'imported' | 'transcribing' | 'transcribed' | 'failed';

export interface ImportFields {
  label: string;
  date?: string | null;
  interviewer?: string | null;
  expected_speakers?: number | null;
  language?: string | null;
  note?: string | null;
}

export interface MediaFile extends ImportFields {
  id: string;
  source: string;
  copied: boolean;
  sha256: string;
  bytes: number;
  duration: number;
  has_video: boolean;
  imported: string;
  status: MediaStatus;
  error?: string | null;
}

export type MediaCheck = { state: 'unchanged' } | { state: 'changed'; sha256: string } | { state: 'missing' };

export interface ProjectView {
  dir: string;
  name: string;
  files: MediaFile[];
  warnings: [string, MediaCheck][];
  ffmpeg: string | null;
}

export interface Word {
  text: string;
  start: number;
  end: number;
  confidence: number;
  speaker: string;
}

export interface Turn {
  speaker: string;
  start: number;
  end: number;
  words: Word[];
}

export interface Speaker {
  id: string;
  label: string;
  name?: string | null;
  role: string;
}

export interface Transcript {
  schema_version: number;
  media: string;
  speakers: Speaker[];
  turns: Turn[];
  review: string;
  provenance: { engine: string; pack: string; created: string; seconds: number };
}

export type Progress =
  | { stage: 'detect_voice' }
  | { stage: 'recognize'; done: number; total: number; seconds: number }
  | { stage: 'label_speakers' }
  | { stage: 'finished' };

export interface JobEvent {
  id: string;
  progress: Progress | null;
  finished: boolean;
  cancelled: boolean;
  error: string | null;
}

export interface ImportEvent {
  path: string;
  done: boolean;
  id: string | null;
  error: string | null;
}

export interface InstallProgress {
  file: string;
  done_bytes: number;
  total_bytes: number;
  pack_done: number;
  pack_total: number;
}

export interface InstallEvent {
  progress: InstallProgress | null;
  finished: boolean;
  error: string | null;
}

export interface ModelStatus {
  installed: boolean;
  message: string;
  dir: string;
  pack: string;
  download_mb: number;
}

export interface Settings {
  offline: boolean;
  model_dir?: string | null;
  ffmpeg?: string | null;
  threads?: number | null;
}

export type Timestamps = { mode: 'per_turn' } | { mode: 'every'; seconds: number } | { mode: 'none' };

export interface ExportOptions {
  timestamps: Timestamps;
  line_numbers: boolean;
}

export const api = {
  projectNew: (dir: string, name: string) => invoke<ProjectView>('project_new', { dir, name }),
  projectOpen: (path: string) => invoke<ProjectView>('project_open', { path }),
  projectView: () => invoke<ProjectView>('project_view'),
  cleanUp: () => invoke<number>('project_clean_up'),
  importMedia: (paths: string[], fields: ImportFields, copy: boolean) => invoke<void>('media_import', { paths, fields, copy }),
  updateMedia: (id: string, fields: ImportFields) => invoke<ProjectView>('media_update', { id, fields }),
  transcribe: (id: string) => invoke<void>('transcribe', { id }),
  cancel: () => invoke<void>('cancel'),
  transcript: (id: string) => invoke<Transcript | null>('transcript', { id }),
  exportFile: (id: string, path: string, options: ExportOptions) => invoke<void>('export', { id, path, options }),
  modelsStatus: () => invoke<ModelStatus>('models_status'),
  modelsInstall: () => invoke<void>('models_install'),
  modelsImportZip: (path: string) => invoke<ModelStatus>('models_import_zip', { path }),
  settings: () => invoke<Settings>('settings_get'),
  saveSettings: (settings: Settings) => invoke<Settings>('settings_set', { settings }),
};

export function on<T>(event: string, handler: (payload: T) => void): Promise<UnlistenFn> {
  return listen<T>(event, (e) => handler(e.payload));
}
