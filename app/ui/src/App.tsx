// Prism Transcribe, phase 1: projects, import, transcribe with progress, a read-only transcript, export, models.
import { useCallback, useEffect, useMemo, useState } from 'react';
import {
  Button,
  Checkbox,
  Dialog,
  Heading,
  Input,
  Label,
  Modal,
  ModalOverlay,
  NumberField,
  ProgressBar,
  Radio,
  RadioGroup,
  Switch,
  TextField,
} from 'react-aria-components';
import { open, save } from '@tauri-apps/plugin-dialog';
import {
  api,
  on,
  type ExportOptions,
  type ImportEvent,
  type ImportFields,
  type InstallEvent,
  type JobEvent,
  type MediaFile,
  type ModelStatus,
  type Progress,
  type ProjectView,
  type Settings,
  type Transcript,
} from './api';
import { duration, hms, LOW_CONFIDENCE, progressText } from './format';
import { Editor } from './editor/Editor';

const MEDIA_EXTENSIONS = ['wav', 'mp3', 'm4a', 'aac', 'flac', 'ogg', 'opus', 'wma', 'mp4', 'mov', 'mkv', 'avi', 'webm'];
const STATUS_WORDS: Record<MediaFile['status'], string> = {
  imported: 'Not transcribed',
  transcribing: 'Transcribing',
  transcribed: 'Transcribed',
  failed: 'Failed',
};

function message(e: unknown): string {
  return typeof e === 'string' ? e : e instanceof Error ? e.message : String(e);
}

export function App() {
  const [project, setProject] = useState<ProjectView | null>(null);
  const [models, setModels] = useState<ModelStatus | null>(null);
  const [settings, setSettings] = useState<Settings>({ offline: false });
  const [selected, setSelected] = useState<string | null>(null);
  const [transcript, setTranscript] = useState<Transcript | null>(null);
  const [job, setJob] = useState<{ id: string; progress: Progress | null } | null>(null);
  const [install, setInstall] = useState<InstallEvent | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [importPaths, setImportPaths] = useState<string[] | null>(null);
  const [importing, setImporting] = useState(0);
  const [newProject, setNewProject] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [editing, setEditing] = useState<{ file: MediaFile; transcript: Transcript } | null>(null);
  const [exporting, setExporting] = useState<MediaFile | null>(null);

  const refresh = useCallback(async () => {
    try {
      setProject(await api.projectView());
    } catch {
      /* no project open yet */
    }
  }, []);

  useEffect(() => {
    api.modelsStatus().then(setModels);
    api.settings().then(setSettings);
    refresh(); // a reloaded window finds the project the app still has open
    const offs = [
      on<JobEvent>('job', (e) => {
        if (e.finished) {
          setJob(null);
          if (e.error) setNotice(`Transcription failed: ${e.error}`);
          else if (e.cancelled) setNotice('Transcription cancelled. Transcribe again to carry on from where it stopped.');
          else setNotice('Transcription finished.');
          refresh();
          if (!e.cancelled && !e.error) api.transcript(e.id).then((t) => { setSelected(e.id); setTranscript(t); });
        } else {
          setJob({ id: e.id, progress: e.progress });
        }
      }),
      on<ImportEvent>('import', (e) => {
        setImporting((n) => Math.max(0, n - 1));
        if (e.error) setNotice(`Could not import ${e.path}: ${e.error}`);
        refresh();
      }),
      on<InstallEvent>('install', (e) => {
        setInstall(e.finished ? null : e);
        if (e.finished) {
          setNotice(e.error ? `The models were not installed: ${e.error}` : 'The speech models are installed and checked.');
          api.modelsStatus().then(setModels);
        }
      }),
    ];
    return () => offs.forEach((p) => p.then((off) => off()));
  }, [refresh]);

  useEffect(() => {
    if (!selected) return setTranscript(null);
    api.transcript(selected).then(setTranscript).catch(() => setTranscript(null));
  }, [selected]);

  const openProject = async () => {
    const dir = await open({ directory: true, title: 'Open a Prism Transcribe project folder' });
    if (typeof dir !== 'string') return;
    try {
      const p = await api.projectOpen(dir);
      setProject(p);
      setSelected(null);
      if (p.warnings.length) {
        const names = p.warnings.map(([id, c]) => `${p.files.find((f) => f.id === id)?.label ?? id} (${c.state === 'missing' ? 'missing' : 'changed since import'})`);
        setNotice(`Check these recordings: ${names.join(', ')}.`);
      }
    } catch (e) {
      setNotice(message(e));
    }
  };

  const startImport = async () => {
    const picked = await open({ multiple: true, title: 'Import recordings', filters: [{ name: 'Audio and video', extensions: MEDIA_EXTENSIONS }] });
    if (!picked) return;
    setImportPaths(Array.isArray(picked) ? picked : [picked]);
  };

  const openEditor = async (f: MediaFile) => {
    const t = await api.transcript(f.id);
    if (t) setEditing({ file: f, transcript: t });
  };

  const exportAs = async (f: MediaFile, kind: 'docx' | 'txt', options: ExportOptions) => {
    const path = await save({
      title: kind === 'docx' ? 'Export to Word' : 'Export to plain text',
      defaultPath: `${f.label}.${kind}`,
      filters: [{ name: kind === 'docx' ? 'Word document' : 'Plain text', extensions: [kind] }],
    });
    if (!path) return;
    try {
      await api.exportFile(f.id, path, options);
      setNotice(`Exported ${f.label} to ${path}.`);
    } catch (e) {
      setNotice(message(e));
    }
  };

  const transcribe = async (f: MediaFile) => {
    try {
      setJob({ id: f.id, progress: null });
      await api.transcribe(f.id);
    } catch (e) {
      setJob(null);
      setNotice(message(e));
    }
  };

  const current = project?.files.find((f) => f.id === selected) ?? null;

  return (
    <div className="app">
      <header className="titlebar">
        <img src="./icon.svg" alt="" className="logo" />
        <h1>Prism Transcribe</h1>
        {project && <span className="project-name">{project.name}</span>}
        <div className="spacer" />
        <Button onPress={() => setNewProject(true)}>New project</Button>
        <Button onPress={openProject}>Open project</Button>
        <Button onPress={() => setSettingsOpen(true)}>Settings</Button>
      </header>

      {notice && (
        <div className="notice" role="status">
          <span>{notice}</span>
          <Button className="quiet" onPress={() => setNotice(null)} aria-label="Dismiss the message">
            Dismiss
          </Button>
        </div>
      )}

      {models && !models.installed && (
        <ModelsPanel models={models} settings={settings} install={install} onNotice={setNotice} onStatus={setModels} />
      )}

      {editing && project ? (
        <Editor
          file={editing.file}
          transcript={editing.transcript}
          onClose={() => {
            setEditing(null);
            setSelected(editing.file.id);
            api.transcript(editing.file.id).then(setTranscript);
          }}
          onSaved={() => {}}
          onNotice={setNotice}
        />
      ) : !project ? (
        <main className="empty">
          <h2>Start with a project</h2>
          <p>A project is one folder that holds your transcripts. The recordings stay where they are unless you choose to copy them in.</p>
          <div className="row">
            <Button className="primary" onPress={() => setNewProject(true)}>New project</Button>
            <Button onPress={openProject}>Open project</Button>
          </div>
        </main>
      ) : (
        <main className="workspace">
          <section className="files" aria-labelledby="files-heading">
            <div className="section-head">
              <h2 id="files-heading">Recordings</h2>
              <Button onPress={startImport}>Import…</Button>
            </div>
            {importing > 0 && <p className="hint" role="status">Importing {importing} file{importing === 1 ? '' : 's'}…</p>}
            {project.files.length === 0 ? (
              <p className="hint">No recordings yet. Import one to begin.</p>
            ) : (
              <ul className="file-list">
                {project.files.map((f) => (
                  <FileRow
                    key={f.id}
                    file={f}
                    selected={f.id === selected}
                    job={job?.id === f.id ? job : null}
                    busy={job !== null}
                    modelsReady={!!models?.installed}
                    onSelect={() => setSelected(f.id)}
                    onTranscribe={() => transcribe(f)}
                    onCancel={() => api.cancel()}
                    onExport={() => setExporting(f)}
                    onEdit={() => openEditor(f)}
                  />
                ))}
              </ul>
            )}
          </section>
          <section className="transcript" aria-labelledby="transcript-heading">
            <h2 id="transcript-heading">{current ? current.label : 'Transcript'}</h2>
            {!current ? (
              <p className="hint">Choose a recording to see its transcript.</p>
            ) : !transcript ? (
              <p className="hint">{current.status === 'failed' ? `This recording could not be transcribed: ${current.error ?? 'unknown reason'}` : 'Not transcribed yet.'}</p>
            ) : (
              <TranscriptView transcript={transcript} />
            )}
          </section>
        </main>
      )}

      <footer className="statusbar">
        <span>{models ? (models.installed ? 'Speech models installed' : 'Speech models not installed') : '…'}</span>
        {settings.offline && (
          <span className="offline" title="Offline: Prism Transcribe makes no network calls">
            <span aria-hidden="true">🔒</span> Offline
          </span>
        )}
        {project && <span>{project.ffmpeg ? 'FFmpeg found: video can be imported' : 'No FFmpeg: video and some audio formats cannot be imported'}</span>}
        {project && (
          <Button className="quiet" isDisabled={job !== null} onPress={async () => setNotice(`Removed ${await api.cleanUp()} temporary file(s).`)}>
            Clean up temporary files
          </Button>
        )}
      </footer>

      <NewProjectDialog
        isOpen={newProject}
        onClose={() => setNewProject(false)}
        onCreated={(p) => {
          setProject(p);
          setSelected(null);
          setNewProject(false);
        }}
        onError={setNotice}
      />
      <ImportDialog
        paths={importPaths}
        onClose={() => setImportPaths(null)}
        onImport={async (fields, copy) => {
          const paths = importPaths ?? [];
          setImportPaths(null);
          setImporting((n) => n + paths.length);
          try {
            await api.importMedia(paths, fields, copy);
          } catch (e) {
            setImporting(0);
            setNotice(message(e));
          }
        }}
      />
      <ExportDialog
        file={exporting}
        onClose={() => setExporting(null)}
        onExport={(kind, opts) => {
          const f = exporting!;
          setExporting(null);
          exportAs(f, kind, opts);
        }}
      />
      <SettingsDialog
        isOpen={settingsOpen}
        settings={settings}
        models={models}
        onClose={() => setSettingsOpen(false)}
        onSave={async (s) => {
          setSettings(await api.saveSettings(s));
          setModels(await api.modelsStatus());
          setSettingsOpen(false);
        }}
      />
    </div>
  );
}

function FileRow(props: {
  file: MediaFile;
  selected: boolean;
  job: { progress: Progress | null } | null;
  busy: boolean;
  modelsReady: boolean;
  onSelect: () => void;
  onTranscribe: () => void;
  onCancel: () => void;
  onExport: () => void;
  onEdit: () => void;
}) {
  const { file: f, job } = props;
  const p = progressText(job?.progress ?? null);
  const done = f.status === 'transcribed';
  return (
    <li className={props.selected ? 'file selected' : 'file'}>
      <Button className="file-main" onPress={props.onSelect} aria-current={props.selected ? 'true' : undefined}>
        <span className="file-label">{f.label}</span>
        <span className="file-meta">
          {duration(f.duration)}
          {f.has_video ? ' · video' : ''}
          {f.expected_speakers ? ` · ${f.expected_speakers} speakers expected` : ''} · {job ? 'Transcribing' : STATUS_WORDS[f.status]}
        </span>
      </Button>
      {job ? (
        <div className="job">
          <ProgressBar aria-label={`Transcribing ${f.label}`} value={p.value === null ? undefined : p.value * 100} isIndeterminate={p.value === null} valueLabel={p.text}>
            {({ percentage }) => (
              <>
                <span className="job-text">{p.text}</span>
                <div className="bar">
                  <div className="fill" style={{ width: p.value === null ? '30%' : `${percentage}%` }} />
                </div>
              </>
            )}
          </ProgressBar>
          <Button onPress={props.onCancel}>Cancel</Button>
        </div>
      ) : (
        <div className="row actions">
          {done && (
            <Button className="primary" onPress={props.onEdit}>
              Edit
            </Button>
          )}
          <Button className={done ? undefined : 'primary'} isDisabled={props.busy || !props.modelsReady || f.status === 'failed'} onPress={props.onTranscribe}>
            {done ? 'Transcribe again' : 'Transcribe'}
          </Button>
          <Button isDisabled={!done} onPress={props.onExport}>Export…</Button>
        </div>
      )}
      {f.status === 'failed' && f.error && <p className="error">{f.error}</p>}
    </li>
  );
}

function TranscriptView({ transcript: t }: { transcript: Transcript }) {
  const names = useMemo(() => new Map(t.speakers.map((s) => [s.id, s.name?.trim() || s.label])), [t]);
  const low = t.turns.reduce((n, turn) => n + turn.words.filter((w) => w.confidence < LOW_CONFIDENCE).length, 0);
  return (
    <>
      <p className="hint">
        {t.turns.length} turns, {t.speakers.length} speaker{t.speakers.length === 1 ? '' : 's'}. Read-only for now; editing comes next.
        {low > 0 && ` ${low} word${low === 1 ? '' : 's'} the recognizer was unsure of ${low === 1 ? 'is' : 'are'} tinted.`}
      </p>
      <div className="turns" role="document" aria-label="Transcript">
        {t.turns.map((turn, i) => (
          <p key={i} className={`turn speaker-${(t.speakers.findIndex((s) => s.id === turn.speaker) % 8) + 1}`}>
            <span className="time">[{hms(turn.start)}]</span> <strong className="who">{names.get(turn.speaker) ?? turn.speaker}:</strong>{' '}
            {turn.words.map((w, j) => (
              <span key={j}>
                {w.confidence < LOW_CONFIDENCE ? (
                  <mark className="unsure" title={`Recognizer was ${Math.round(w.confidence * 100)}% sure`}>
                    {w.text}
                  </mark>
                ) : (
                  w.text
                )}{' '}
              </span>
            ))}
          </p>
        ))}
      </div>
    </>
  );
}

function ModelsPanel(props: { models: ModelStatus; settings: Settings; install: InstallEvent | null; onNotice: (s: string) => void; onStatus: (m: ModelStatus) => void }) {
  const pr = props.install?.progress;
  return (
    <section className="models" aria-labelledby="models-heading">
      <h2 id="models-heading">Speech models</h2>
      <p>
        Prism Transcribe needs its speech models once: about {props.models.download_mb} MB to download, kept in <code>{props.models.dir}</code> and
        shared with other suite tools. They run on this computer; nothing you record is sent anywhere.
      </p>
      {pr ? (
        <ProgressBar aria-label="Downloading the speech models" value={(pr.pack_done / pr.pack_total) * 100}>
          {({ percentage }) => (
            <>
              <span>
                Downloading {Math.round(pr.pack_done / 1e6)} of {Math.round(pr.pack_total / 1e6)} MB ({Math.round(percentage ?? 0)}%)
              </span>
              <div className="bar">
                <div className="fill" style={{ width: `${percentage}%` }} />
              </div>
            </>
          )}
        </ProgressBar>
      ) : (
        <div className="row">
          <Button
            className="primary"
            isDisabled={props.settings.offline}
            onPress={() => api.modelsInstall().catch((e) => props.onNotice(message(e)))}
          >
            Download the speech models
          </Button>
          <Button
            onPress={async () => {
              const z = await open({ title: 'Import the speech models from an offline zip', filters: [{ name: 'Zip', extensions: ['zip'] }] });
              if (typeof z !== 'string') return;
              try {
                props.onStatus(await api.modelsImportZip(z));
                props.onNotice('The speech models are installed and checked.');
              } catch (e) {
                props.onNotice(message(e));
              }
            }}
          >
            Import from an offline zip…
          </Button>
          {props.settings.offline && <span className="hint">Offline is on, so nothing can be downloaded. Turn it off in Settings, or use an offline zip.</span>}
        </div>
      )}
    </section>
  );
}

function NewProjectDialog(props: { isOpen: boolean; onClose: () => void; onCreated: (p: ProjectView) => void; onError: (s: string) => void }) {
  const [name, setName] = useState('');
  const [dir, setDir] = useState('');
  useEffect(() => {
    if (props.isOpen) {
      setName('');
      setDir('');
    }
  }, [props.isOpen]);
  return (
    <ModalOverlay isOpen={props.isOpen} onOpenChange={(o) => !o && props.onClose()} isDismissable>
      <Modal>
        <Dialog>
          <Heading slot="title">New project</Heading>
          <TextField value={name} onChange={setName} autoFocus>
            <Label>Name</Label>
            <Input placeholder="Teacher interviews, spring" />
          </TextField>
          <div className="row">
            <span className="path">{dir || 'No folder chosen'}</span>
            <Button
              onPress={async () => {
                const d = await open({ directory: true, title: 'Choose a folder for the project (an empty one is best)' });
                if (typeof d === 'string') setDir(d);
              }}
            >
              Choose folder…
            </Button>
          </div>
          <div className="row end">
            <Button onPress={props.onClose}>Cancel</Button>
            <Button
              className="primary"
              isDisabled={!name.trim() || !dir}
              onPress={async () => {
                try {
                  props.onCreated(await api.projectNew(dir, name.trim()));
                } catch (e) {
                  props.onError(message(e));
                }
              }}
            >
              Create
            </Button>
          </div>
        </Dialog>
      </Modal>
    </ModalOverlay>
  );
}

function ImportDialog(props: { paths: string[] | null; onClose: () => void; onImport: (f: ImportFields, copy: boolean) => void }) {
  const [label, setLabel] = useState('');
  const [date, setDate] = useState('');
  const [interviewer, setInterviewer] = useState('');
  const [speakers, setSpeakers] = useState<number>(NaN);
  const [note, setNote] = useState('');
  const [copy, setCopy] = useState(false);
  const many = (props.paths?.length ?? 0) > 1;
  useEffect(() => {
    if (props.paths) {
      setLabel('');
      setDate('');
      setInterviewer('');
      setSpeakers(NaN);
      setNote('');
      setCopy(false);
    }
  }, [props.paths]);
  return (
    <ModalOverlay isOpen={!!props.paths} onOpenChange={(o) => !o && props.onClose()} isDismissable>
      <Modal>
        <Dialog>
          <Heading slot="title">Import {many ? `${props.paths?.length} recordings` : 'a recording'}</Heading>
          <p className="hint">Every field is optional. {many ? 'Each file is labelled with its file name; the rest applies to all of them.' : ''}</p>
          {!many && (
            <TextField value={label} onChange={setLabel}>
              <Label>Label</Label>
              <Input placeholder="INT-07 (the file name if left empty)" />
            </TextField>
          )}
          <TextField value={date} onChange={setDate}>
            <Label>Date recorded</Label>
            <Input placeholder="2026-03-04" />
          </TextField>
          <TextField value={interviewer} onChange={setInterviewer}>
            <Label>Interviewer</Label>
            <Input />
          </TextField>
          <NumberField value={speakers} onChange={setSpeakers} minValue={1} maxValue={20}>
            <Label>Expected number of speakers</Label>
            <Input placeholder="Found automatically if left empty" />
          </NumberField>
          <TextField value={note} onChange={setNote}>
            <Label>Note</Label>
            <Input />
          </TextField>
          <Checkbox isSelected={copy} onChange={setCopy}>
            <span className="box" aria-hidden="true" /> Copy the recording{many ? 's' : ''} into the project folder (for archiving)
          </Checkbox>
          <div className="row end">
            <Button onPress={props.onClose}>Cancel</Button>
            <Button
              className="primary"
              onPress={() =>
                props.onImport(
                  {
                    label: label.trim(),
                    date: date.trim() || null,
                    interviewer: interviewer.trim() || null,
                    expected_speakers: Number.isFinite(speakers) ? speakers : null,
                    note: note.trim() || null,
                  },
                  copy,
                )
              }
            >
              Import
            </Button>
          </div>
        </Dialog>
      </Modal>
    </ModalOverlay>
  );
}

function SettingsDialog(props: { isOpen: boolean; settings: Settings; models: ModelStatus | null; onClose: () => void; onSave: (s: Settings) => void }) {
  const [s, setS] = useState<Settings>(props.settings);
  const [threads, setThreads] = useState('auto');
  useEffect(() => {
    if (props.isOpen) {
      setS(props.settings);
      setThreads(props.settings.threads ? String(props.settings.threads) : 'auto');
    }
  }, [props.isOpen, props.settings]);
  return (
    <ModalOverlay isOpen={props.isOpen} onOpenChange={(o) => !o && props.onClose()} isDismissable>
      <Modal>
        <Dialog>
          <Heading slot="title">Settings</Heading>
          <Switch isSelected={s.offline} onChange={(v) => setS({ ...s, offline: v })}>
            <span className="switch" aria-hidden="true" /> Offline: make no network calls at all
          </Switch>
          <p className="hint">
            Prism Transcribe's only network call is the speech-model download you start yourself. Offline blocks it too. Recordings and
            transcripts never leave this computer either way.
          </p>
          <RadioGroup value={threads} onChange={setThreads}>
            <Label>Processor threads for transcription</Label>
            <Radio value="auto">Automatic (up to 8)</Radio>
            <Radio value="4">4</Radio>
            <Radio value="8">8</Radio>
            <Radio value="16">16</Radio>
          </RadioGroup>
          {props.models && <p className="hint">Speech models: {props.models.dir}</p>}
          <section className="about" aria-labelledby="about-heading">
            <h3 id="about-heading">About and credits</h3>
            <p>Prism Transcribe is free software under the Apache License 2.0. It runs these models on this computer:</p>
            <ul>
              <li>Parakeet TDT 0.6B v3 by NVIDIA, licensed under CC BY 4.0, converted for sherpa-onnx.</li>
              <li>WeSpeaker ResNet34-LM speaker embedding (WeSpeaker project, trained on VoxCeleb), licensed under CC BY 4.0, converted for sherpa-onnx; Prism Transcribe adds one setting to the file so each recording's average is removed.</li>
              <li>Silero VAD by the Silero Team, MIT licence.</li>
              <li>sherpa-onnx (Apache License 2.0) and ONNX Runtime (MIT).</li>
            </ul>
          </section>
          <div className="row end">
            <Button onPress={props.onClose}>Cancel</Button>
            <Button className="primary" onPress={() => props.onSave({ ...s, threads: threads === 'auto' ? null : Number(threads) })}>
              Save
            </Button>
          </div>
        </Dialog>
      </Modal>
    </ModalOverlay>
  );
}

function ExportDialog(props: { file: MediaFile | null; onClose: () => void; onExport: (kind: 'docx' | 'txt', o: ExportOptions) => void }) {
  const [kind, setKind] = useState<'docx' | 'txt'>('docx');
  const [stamps, setStamps] = useState('per_turn');
  const [clean, setClean] = useState(false);
  const [lines, setLines] = useState(false);
  return (
    <ModalOverlay isOpen={!!props.file} onOpenChange={(o) => !o && props.onClose()} isDismissable>
      <Modal>
        <Dialog>
          <Heading slot="title">Export {props.file?.label}</Heading>
          <RadioGroup value={kind} onChange={(v) => setKind(v as 'docx' | 'txt')}>
            <Label>Format</Label>
            <Radio value="docx">Word (.docx)</Radio>
            <Radio value="txt">Plain text (.txt)</Radio>
          </RadioGroup>
          <RadioGroup value={stamps} onChange={setStamps}>
            <Label>Timestamps</Label>
            <Radio value="per_turn">At the start of every turn</Radio>
            <Radio value="every">Every 30 seconds</Radio>
            <Radio value="none">None</Radio>
          </RadioGroup>
          <RadioGroup value={clean ? 'clean' : 'verbatim'} onChange={(v) => setClean(v === 'clean')}>
            <Label>Text</Label>
            <Radio value="verbatim">Verbatim: every word as said</Radio>
            <Radio value="clean">Clean: fillers and repeated words left out</Radio>
          </RadioGroup>
          {kind === 'docx' && (
            <Checkbox isSelected={lines} onChange={setLines}>
              <span className="box" aria-hidden="true" /> Number the lines
            </Checkbox>
          )}
          <div className="row end">
            <Button onPress={props.onClose}>Cancel</Button>
            <Button
              className="primary"
              onPress={() =>
                props.onExport(kind, {
                  timestamps: stamps === 'every' ? { mode: 'every', seconds: 30 } : stamps === 'none' ? { mode: 'none' } : { mode: 'per_turn' },
                  line_numbers: kind === 'docx' && lines,
                  clean,
                })
              }
            >
              Choose where to save…
            </Button>
          </div>
        </Dialog>
      </Modal>
    </ModalOverlay>
  );
}
