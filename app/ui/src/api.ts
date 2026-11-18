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
  /** Password protection is on, and whether this session has not unlocked it yet. */
  protected: boolean;
  locked: boolean;
  word_list: string[];
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
  /** Captions: show who is speaking. */
  speaker_names: boolean;
}

// ---- De-identification (T6) ----

export type NameKind = 'person' | 'place' | 'organisation' | 'other';

export interface NameEntry {
  real: string;
  variants: string[];
  replacement: string;
  kind: NameKind;
  source: 'typed' | 'roster' | 'suggestion';
}

export interface LogEntry {
  when: string;
  file: string;
  label: string;
  place: string;
  at?: number | null;
  original: string;
  replacement: string;
  reason: string;
  by: string;
}

export type PatternKind = 'email' | 'web_address' | 'phone' | 'address' | 'date' | 'birth' | 'id_number';

export interface Flag {
  file: string;
  label: string;
  turn: number;
  start: number;
  end: number;
  text: string;
  kind: PatternKind;
  at: number;
  before: string;
  after: string;
}

export interface Suggestion {
  word: string;
  likelihood: 'likely' | 'possible';
  count: number;
  files: string[];
  examples: [string, number, string, string][];
}

export interface Leftover {
  place: string;
  text: string;
  at?: number | null;
}

export interface FileCheck {
  file: string;
  label: string;
  review: ReviewStatus;
  transcribed: boolean;
  leftovers: Leftover[];
  snapshots_with_names: number;
}

export type KeyState =
  | { state: 'none' }
  | { state: 'missing'; path: string }
  | { state: 'unreadable'; path: string; message: string }
  | { state: 'ready'; path: string; names: number; changes: number; inside_project: boolean };

export interface FileCount {
  file: string;
  label: string;
  count: number;
}

// ---- The batch queue (T8) ----

export interface QueueItem {
  id: string;
  state: 'waiting' | 'running' | 'held' | 'done' | 'failed' | 'cancelled';
  error?: string | null;
  added: string;
  finished?: string | null;
}

export interface Queue {
  state: 'idle' | 'running' | 'paused';
  items: QueueItem[];
  interrupted?: boolean;
  /** Raised by every change: a lower one arriving late is older. */
  rev?: number;
}

export interface QueueSummary {
  done: string[];
  failed: [string, string][];
  seconds: number;
}

export interface QueueEvent {
  queue: Queue;
  summary: QueueSummary | null;
}

export interface ExportCheck {
  key: KeyState;
  files: { id: string; label: string; deidentified: boolean; check: FileCheck | null }[];
}

export interface UpdateState {
  version: string;
  set_up: boolean;
  host: string;
  blocked: 'not_set_up' | 'offline' | null;
}

export type UpdateFound = { available: false } | { available: true; version: string; notes: string; date: string };

export interface SuiteApps {
  prismqda: string | null;
  hub: string | null;
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
  exportFile: (ids: string[], path: string, options: ExportOptions, deidentified: boolean) => invoke<void>('export', { ids, path, options, deidentified }),
  exportCheck: (ids: string[]) => invoke<ExportCheck>('export_check', { ids }),
  suiteApps: () => invoke<SuiteApps>('suite_apps'),
  updateState: () => invoke<UpdateState>('update_state'),
  updateCheck: () => invoke<UpdateFound>('update_check'),
  updateInstall: () => invoke<{ installed: boolean; version?: string }>('update_install'),
  suiteOpen: (app: 'prismqda' | 'hub') => invoke<void>('suite_open', { app }),
  queue: () => invoke<Queue>('queue_get'),
  queueAdd: (ids: string[], start: boolean) => invoke<Queue>('queue_add', { ids, start }),
  queueStart: () => invoke<Queue>('queue_start'),
  queuePause: () => invoke<Queue>('queue_pause'),
  queueCancel: () => invoke<Queue>('queue_cancel'),
  queueHold: (id: string, hold: boolean) => invoke<Queue>('queue_hold', { id, hold }),
  queueRemove: (id: string) => invoke<Queue>('queue_remove', { id }),
  queueMove: (id: string, to: number) => invoke<Queue>('queue_move', { id, to }),
  queueClear: () => invoke<Queue>('queue_clear'),
  keyState: () => invoke<KeyState>('deid_key_state'),
  keyUse: (path: string) => invoke<KeyState>('deid_key_use', { path }),
  names: () => invoke<NameEntry[]>('deid_names'),
  setNames: (names: NameEntry[]) => invoke<NameEntry[]>('deid_names_set', { names }),
  pasteNames: (text: string) => invoke<NameEntry[]>('deid_names_paste', { text }),
  autoVariants: (real: string, kind: NameKind) => invoke<string[]>('deid_auto_variants', { real, kind }),
  namesPreview: () => invoke<FileCount[]>('deid_names_preview'),
  namesApply: () => invoke<FileCount[]>('deid_names_apply'),
  flags: () => invoke<Flag[]>('deid_flags'),
  flagApply: (flag: Flag, replacement: string) => invoke<boolean>('deid_flag_apply', { flag, replacement }),
  flagSkip: (text: string) => invoke<void>('deid_flag_skip', { text }),
  suggestions: () => invoke<{ list: Suggestion[]; dictionary: string | null }>('deid_suggestions'),
  suggestionAccept: (word: string, replacement: string, kind: NameKind) => invoke<FileCount[]>('deid_suggestion_accept', { word, replacement, kind }),
  suggestionSkip: (word: string) => invoke<string[]>('deid_suggestion_skip', { word }),
  setWordList: (words: string[]) => invoke<string[]>('word_list_set', { words }),
  finalCheck: () => invoke<FileCheck[]>('deid_check'),
  markDeidentified: (id: string) => invoke<FileCheck>('deid_mark', { id }),
  deidLog: () => invoke<LogEntry[]>('deid_log'),
  setPassword: (password: string | null) => invoke<ProjectView>('project_password_set', { password }),
  unlock: (password: string) => invoke<ProjectView>('project_unlock', { password }),
  lock: () => invoke<ProjectView>('project_lock'),
  statementText: () => invoke<string>('statement_text'),
  statementExport: (path: string) => invoke<void>('statement_export', { path }),
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
