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
  /** The Advanced panel: 'light-1' or 'heavy-1'; empty picks by language. */
  pack?: string | null;
  /** One microphone per person: transcribe each track, or each channel, as one known speaker. */
  tracks?: TrackMode;
}

export type TrackMode = 'mixed' | 'tracks' | 'channels';

export interface MediaFile extends ImportFields {
  id: string;
  source: string;
  copied: boolean;
  sha256: string;
  bytes: number;
  duration: number;
  has_video: boolean;
  audio_tracks?: number;
  channels?: number;
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

export type Role = 'interviewer' | 'moderator' | 'participant' | 'other';

export interface Speaker {
  id: string;
  label: string;
  name?: string | null;
  role: Role;
  attributes?: Record<string, string>;
}

export interface SpeakerStats {
  id: string;
  turns: number;
  words: number;
  seconds: number;
}

export type SpeakerOp =
  | { op: 'split'; turn: number; word: number }
  | { op: 'reassign'; turn: number; from?: number | null; to?: number | null; speaker: string }
  | { op: 'join_previous'; turn: number }
  | { op: 'merge'; from: string; into: string }
  | { op: 'add' }
  | { op: 'remove'; id: string };

export interface SpeakerEdit {
  speakers: Speaker[];
  turns: Turn[];
  turn: number | null;
  changed: boolean;
}

export type ReviewStatus = 'not_started' | 'first_pass' | 'checked' | 'de_identified' | 'final';

export interface Snapshot {
  name: string;
  created: string;
  turns: Turn[];
}

export interface Transcript {
  schema_version: number;
  media: string;
  speakers: Speaker[];
  turns: Turn[];
  notation?: unknown[];
  snapshots: Snapshot[];
  review: ReviewStatus;
  checked_until?: number | null;
  provenance: { engine: string; pack: string; created: string; seconds: number; expected_speakers?: number | null };
}

export interface EditorInfo {
  url: string;
  playback_copy: boolean;
  has_video: boolean;
  duration: number;
  peaks_per_second: number;
  notation: string[];
  hotkeys: Record<string, string>;
  rewind_on_resume: number;
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

export interface PackStatus {
  id: string;
  name: string;
  installed: boolean;
  message: string;
  download_mb: number;
  program?: string | null;
}

export interface ModelStatus {
  installed: boolean;
  message: string;
  dir: string;
  pack: string;
  download_mb: number;
  heavy: PackStatus;
}

export interface Settings {
  offline: boolean;
  model_dir?: string | null;
  ffmpeg?: string | null;
  threads?: number | null;
  whisper?: string | null;
}

export type Timestamps = { mode: 'per_turn' } | { mode: 'every'; seconds: number } | { mode: 'none' };

export interface ExportOptions {
  timestamps: Timestamps;
  line_numbers: boolean;
  clean: boolean;
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
  modelsInstall: (pack?: string) => invoke<void>('models_install', { pack: pack ?? null }),
  modelsImportZip: (path: string, pack?: string) => invoke<ModelStatus>('models_import_zip', { path, pack: pack ?? null }),
  speakersEdit: (op: SpeakerOp, speakers: Speaker[], turns: Turn[]) => invoke<SpeakerEdit>('speakers_edit', { op, speakers, turns }),
  speakersStats: (speakers: Speaker[], turns: Turn[]) => invoke<SpeakerStats[]>('speakers_stats', { speakers, turns }),
  findSpeakersAgain: (id: string, turns: Turn[], expected: number | null) => invoke<SpeakerEdit>('speakers_find_again', { id, turns, expected }),
  editorInfo: (id: string) => invoke<EditorInfo>('editor_info', { id }),
  peaks: (id: string) => invoke<number[]>('peaks', { id }),
  retimeTurn: (turn: Turn, text: string, lo: number, hi: number) => invoke<Turn>('retime_turn', { turn, text, lo, hi }),
  cleanMasks: (turns: Turn[]) => invoke<boolean[][]>('clean_masks', { turns }),
  saveTranscript: (transcript: Transcript) => invoke<void>('transcript_save', { transcript }),
  replaceInProject: (find: string, replace: string, options: { whole_word: boolean; match_case: boolean }) =>
    invoke<[string, number][]>('replace_in_project', { find, replace, options }),
  setNotation: (list: string[]) => invoke<string[]>('notation_set', { list }),
  setHotkeys: (hotkeys: Record<string, string>, rewind_on_resume: number) => invoke<Record<string, string>>('hotkeys_set', { hotkeys, rewind_on_resume }),
  defaultHotkeys: () => invoke<Record<string, string>>('hotkeys_default'),
  settings: () => invoke<Settings>('settings_get'),
  saveSettings: (settings: Settings) => invoke<Settings>('settings_set', { settings }),
};

export function on<T>(event: string, handler: (payload: T) => void): Promise<UnlistenFn> {
  return listen<T>(event, (e) => handler(e.payload));
}
