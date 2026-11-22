// De-identification (T6): the key file, the name list, the pattern and suggestion passes, the final check and the
// change log. Every change made here is logged in the key file, which is kept apart from the transcripts.
import { useCallback, useEffect, useState } from 'react';
import { Button, Dialog, Heading, Label, Modal, ModalOverlay, Tab, TabList, TabPanel, Tabs, TextArea, TextField } from 'react-aria-components';
import { open, save } from '@tauri-apps/plugin-dialog';
import { api, type FileCheck, type FileCount, type Flag, type KeyState, type LogEntry, type NameEntry, type NameKind, type PatternKind, type ProjectView, type Suggestion } from './api';
import { hms, errorText } from './format';

const KIND_WORDS: Record<NameKind, string> = { person: 'Person', place: 'Place', organisation: 'Organisation', other: 'Other' };

const PATTERN_WORDS: Record<PatternKind, string> = {
  email: 'Email address',
  web_address: 'Web address',
  phone: 'Phone number',
  address: 'Address or postcode',
  date: 'Date',
  birth: 'Birth date or place',
  id_number: 'ID-like number',
};

const PATTERN_REPLACEMENT: Record<PatternKind, string> = {
  email: '[email address]',
  web_address: '[web address]',
  phone: '[phone number]',
  address: '[address]',
  date: '[date]',
  birth: '[birth details]',
  id_number: '[ID number]',
};

const REVIEW_WORDS: Record<string, string> = { not_started: 'Not started', first_pass: 'First pass', checked: 'Checked', de_identified: 'De-identified', final: 'Final' };

function message(e: unknown): string {
  return errorText(e);
}

function defaultReplacement(kind: NameKind, names: NameEntry[]): string {
  if (kind === 'person') return `[Person ${names.filter((n) => n.kind === 'person').length + 1}]`;
  return kind === 'place' ? '[Place]' : kind === 'organisation' ? '[Organisation]' : '[redacted]';
}

function counted(list: FileCount[]): string {
  const n = list.reduce((a, f) => a + f.count, 0);
  return `${n} replacement${n === 1 ? '' : 's'} in ${list.length} file${list.length === 1 ? '' : 's'}`;
}

// ---------------------------------------------------------------------------------------------------------
// The key file
// ---------------------------------------------------------------------------------------------------------

function KeyChooser(props: { project: ProjectView; state: KeyState; onState: (k: KeyState) => void; onNotice: (s: string) => void }) {
  const choose = async (existing: boolean) => {
    const path = existing
      ? await open({ title: 'Open the key file', filters: [{ name: 'Prism Transcribe key file', extensions: ['prismkey'] }] })
      : await save({ title: 'Where should the key file go?', defaultPath: `${props.project.name} key.prismkey`, filters: [{ name: 'Prism Transcribe key file', extensions: ['prismkey'] }] });
    if (typeof path !== 'string') return;
    try {
      props.onState(await api.keyUse(path));
    } catch (e) {
      props.onNotice(message(e));
    }
  };
  const s = props.state;
  return (
    <section className="key-chooser" aria-labelledby="key-heading">
      <h3 id="key-heading">The key file</h3>
      {s.state === 'none' && (
        <p>
          De-identification keeps a <strong>key file</strong>: the real names, what each is replaced with, and a log of every change. It lets you
          trace a pseudonym back, so keep it apart from the transcripts: on an encrypted drive or a protected network folder, not in the project
          folder. It is never included in an export.
        </p>
      )}
      {s.state === 'missing' && (
        <p role="alert">
          The key file is not at <span className="path">{s.path}</span>. If it is on a removable drive, plug it in and try again, or find it where it
          was moved.
        </p>
      )}
      {s.state === 'unreadable' && (
        <p role="alert">
          The key file at <span className="path">{s.path}</span> cannot be used: {s.message}.
        </p>
      )}
      <div className="row">
        <Button className="primary" onPress={() => choose(false)}>
          {s.state === 'none' ? 'Choose where to save the key file…' : 'Start a new key file…'}
        </Button>
        <Button onPress={() => choose(true)}>{s.state === 'none' ? 'Open an existing key file…' : 'Find the key file…'}</Button>
        {s.state === 'missing' && (
          <Button onPress={async () => props.onState(await api.keyState())}>Try again</Button>
        )}
      </div>
    </section>
  );
}

// ---------------------------------------------------------------------------------------------------------
// Names
// ---------------------------------------------------------------------------------------------------------

function NamesTab(props: { onNotice: (s: string) => void; onChanged: () => void }) {
  const [names, setNames] = useState<NameEntry[]>([]);
  const [pasting, setPasting] = useState(false);
  const [roster, setRoster] = useState('');
  const [preview, setPreview] = useState<FileCount[] | null>(null);
  const [working, setWorking] = useState(false);
  // Older key files may leave out an empty list of spellings.
  const tidy = (list: NameEntry[]) => list.map((n) => ({ ...n, variants: n.variants ?? [] }));
  useEffect(() => {
    api.names().then((l) => setNames(tidy(l))).catch((e) => props.onNotice(message(e)));
  }, []);
  const persist = async (list: NameEntry[]) => {
    try {
      setNames(tidy(await api.setNames(list)));
      props.onChanged();
    } catch (e) {
      props.onNotice(message(e));
    }
  };
  const edit = (i: number, change: Partial<NameEntry>) => setNames(names.map((n, j) => (j === i ? { ...n, ...change } : n)));
  // A new name's usual spellings and replacement are filled in when its name is first written.
  const finishReal = async (i: number) => {
    const n = names[i];
    if (!n.real.trim()) return;
    const list = [...names];
    const fill: Partial<NameEntry> = {};
    if (n.variants.length === 0) fill.variants = await api.autoVariants(n.real.trim(), n.kind);
    if (!n.replacement.trim()) fill.replacement = defaultReplacement(n.kind, names.filter((_, j) => j !== i));
    list[i] = { ...n, ...fill };
    persist(list);
  };
  return (
    <div className="deid-tab">
      <p className="hint">
        Real names and places, each with the other ways it is said, and what replaces it. Replacing ignores case and keeps possessives: with
        Maria → P07, "maria's" becomes "P07's". Longer names go first, so "Maria Lopez" is replaced whole.
      </p>
      {names.length > 0 && (
        <table className="names-table">
          <thead>
            <tr>
              <th scope="col">Real name</th>
              <th scope="col">Other spellings (commas between)</th>
              <th scope="col">Replaced with</th>
              <th scope="col">Kind</th>
              <th scope="col">
                <span className="visually-hidden">Remove</span>
              </th>
            </tr>
          </thead>
          <tbody>
            {names.map((n, i) => (
              <tr key={i}>
                <td>
                  <input aria-label={`Real name ${i + 1}`} value={n.real} onChange={(e) => edit(i, { real: e.target.value })} onBlur={() => finishReal(i)} />
                </td>
                <td>
                  <input
                    aria-label={`Other spellings of ${n.real || `name ${i + 1}`}`}
                    value={n.variants.join(', ')}
                    onChange={(e) => edit(i, { variants: e.target.value.split(',').map((v) => v.trimStart()) })}
                    onBlur={() => persist(names.map((x, j) => (j === i ? { ...x, variants: x.variants.map((v) => v.trim()).filter(Boolean) } : x)))}
                  />
                </td>
                <td>
                  <input aria-label={`Replacement for ${n.real || `name ${i + 1}`}`} value={n.replacement} onChange={(e) => edit(i, { replacement: e.target.value })} onBlur={() => persist(names)} />
                </td>
                <td>
                  <select aria-label={`Kind of ${n.real || `name ${i + 1}`}`} value={n.kind} onChange={(e) => persist(names.map((x, j) => (j === i ? { ...x, kind: e.target.value as NameKind } : x)))}>
                    {Object.entries(KIND_WORDS).map(([k, v]) => (
                      <option key={k} value={k}>
                        {v}
                      </option>
                    ))}
                  </select>
                </td>
                <td>
                  <Button className="quiet" aria-label={`Remove ${n.real || `name ${i + 1}`}`} onPress={() => persist(names.filter((_, j) => j !== i))}>
                    Remove
                  </Button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      <div className="row">
        <Button onPress={() => setNames([...names, { real: '', variants: [], replacement: '', kind: 'person', source: 'typed' }])}>Add a name</Button>
        <Button onPress={() => setPasting(true)}>Paste a roster…</Button>
        <span className="spacer" />
        <Button
          className="primary"
          isDisabled={names.length === 0 || working}
          onPress={async () => {
            try {
              await persist(names);
              setPreview(await api.namesPreview());
            } catch (e) {
              props.onNotice(message(e));
            }
          }}
        >
          Replace names in all transcripts…
        </Button>
      </div>

      <ModalOverlay isOpen={pasting} onOpenChange={(o) => !o && setPasting(false)} isDismissable>
        <Modal className="react-aria-Modal wide">
          <Dialog>
            <Heading slot="title">Paste a roster</Heading>
            <p className="hint">
              One name per line, from a spreadsheet or a list. A replacement can follow after a tab, "=", or "-&gt;" (Maria Lopez = P07), and a kind
              after that (person, place, organisation). "Lopez, Maria" is read as Maria Lopez. Names without a replacement get the next [Person n].
            </p>
            <TextField value={roster} onChange={setRoster}>
              <Label>Names</Label>
              <TextArea rows={10} className="roster" />
            </TextField>
            <div className="row end">
              <Button onPress={() => setPasting(false)}>Cancel</Button>
              <Button
                className="primary"
                isDisabled={!roster.trim()}
                onPress={async () => {
                  try {
                    const before = names.length;
                    const list = tidy(await api.pasteNames(roster));
                    setNames(list);
                    props.onChanged();
                    setPasting(false);
                    setRoster('');
                    props.onNotice(`Added ${list.length - before} name${list.length - before === 1 ? '' : 's'} to the list.`);
                  } catch (e) {
                    props.onNotice(message(e));
                  }
                }}
              >
                Add to the list
              </Button>
            </div>
          </Dialog>
        </Modal>
      </ModalOverlay>

      <ModalOverlay isOpen={!!preview} onOpenChange={(o) => !o && setPreview(null)} isDismissable={!working}>
        <Modal>
          <Dialog>
            <Heading slot="title">Replace names</Heading>
            {preview && preview.every((f) => f.count === 0) ? (
              <p>No name on the list is left in any transcript.</p>
            ) : (
              <>
                <p>The list would make these replacements. Each is logged in the key file; a copy of each transcript as it was is kept as the snapshot "Before de-identification".</p>
                <ul>
                  {preview
                    ?.filter((f) => f.count > 0)
                    .map((f) => (
                      <li key={f.file}>
                        {f.label}: {f.count}
                      </li>
                    ))}
                </ul>
              </>
            )}
            <div className="row end">
              <Button onPress={() => setPreview(null)} isDisabled={working}>
                {preview?.some((f) => f.count > 0) ? 'Cancel' : 'Close'}
              </Button>
              {preview?.some((f) => f.count > 0) && (
                <Button
                  className="primary"
                  isDisabled={working}
                  onPress={async () => {
                    setWorking(true);
                    try {
                      const done = await api.namesApply();
                      props.onNotice(`Made ${counted(done)}. Each change is in the key file's log.`);
                      props.onChanged();
                    } catch (e) {
                      props.onNotice(message(e));
                    } finally {
                      setWorking(false);
                      setPreview(null);
                    }
                  }}
                >
                  {working ? 'Replacing…' : 'Replace them'}
                </Button>
              )}
            </div>
          </Dialog>
        </Modal>
      </ModalOverlay>
    </div>
  );
}

// ---------------------------------------------------------------------------------------------------------
// Patterns
// ---------------------------------------------------------------------------------------------------------

function flagKey(f: Flag): string {
  return `${f.file}:${f.turn}:${f.start}`;
}

function PatternsTab(props: { onNotice: (s: string) => void; onChanged: () => void }) {
  const [flags, setFlags] = useState<Flag[] | null>(null);
  const [reps, setReps] = useState<Record<string, string>>({});
  const [busy, setBusy] = useState(false);
  const find = useCallback(async () => {
    setBusy(true);
    try {
      setFlags(await api.flags());
    } catch (e) {
      props.onNotice(message(e));
    } finally {
      setBusy(false);
    }
  }, []);
  useEffect(() => {
    find();
  }, [find]);
  return (
    <div className="deid-tab">
      <p className="hint">
        Email addresses, phone numbers, street addresses and postcodes, dates, birth details, ID-like numbers and web addresses, written or spoken
        ("maria at example dot com"). Accept replaces one; Skip leaves it, and the same text is not flagged again in this project.
      </p>
      <div className="row">
        <Button onPress={find} isDisabled={busy}>
          {busy ? 'Looking…' : 'Look again'}
        </Button>
        {flags && <span role="status">{flags.length === 0 ? 'Nothing found to review.' : `${flags.length} to review.`}</span>}
      </div>
      {flags && flags.length > 0 && (
        <ul className="review-list">
          {flags.map((f) => {
            const k = flagKey(f);
            const rep = reps[k] ?? PATTERN_REPLACEMENT[f.kind];
            return (
              <li key={k} className="review-item">
                <div className="review-head">
                  <strong>{PATTERN_WORDS[f.kind]}</strong>
                  <span className="hint">
                    {f.label} at {hms(f.at)}
                  </span>
                </div>
                <p className="context">
                  {f.before}
                  <mark>{f.text}</mark>
                  {f.after}
                </p>
                <div className="row">
                  <input aria-label={`Replace ${f.text} with`} value={rep} onChange={(e) => setReps({ ...reps, [k]: e.target.value })} />
                  <Button
                    className="primary"
                    onPress={async () => {
                      try {
                        const ok = await api.flagApply(f, rep);
                        if (!ok) props.onNotice('That transcript changed since the list was made; the list is fresh now.');
                        props.onChanged();
                        await find();
                      } catch (e) {
                        props.onNotice(message(e));
                      }
                    }}
                  >
                    Accept
                  </Button>
                  <Button
                    onPress={async () => {
                      try {
                        await api.flagSkip(f.text);
                        setFlags((l) => (l ?? []).filter((x) => x.text.toLowerCase() !== f.text.toLowerCase()));
                      } catch (e) {
                        props.onNotice(message(e));
                      }
                    }}
                  >
                    Skip
                  </Button>
                </div>
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------------------------------------
// Suggestions
// ---------------------------------------------------------------------------------------------------------

function SuggestionsTab(props: { project: ProjectView; onNotice: (s: string) => void; onChanged: () => void }) {
  const [list, setList] = useState<Suggestion[] | null>(null);
  const [dictionary, setDictionary] = useState<string | null>(null);
  const [reps, setReps] = useState<Record<string, { replacement: string; kind: NameKind }>>({});
  const [busy, setBusy] = useState(false);
  const [words, setWords] = useState(props.project.word_list.join('\n'));
  useEffect(() => setWords(props.project.word_list.join('\n')), [props.project.word_list]);
  const find = useCallback(async () => {
    setBusy(true);
    try {
      const r = await api.suggestions();
      setList(r.list);
      setDictionary(r.dictionary);
    } catch (e) {
      props.onNotice(message(e));
    } finally {
      setBusy(false);
    }
  }, []);
  useEffect(() => {
    find();
  }, [find]);
  return (
    <div className="deid-tab">
      <p className="hint">
        Capitalised words that may be names: not in the dictionary, or capitalised in the middle of a sentence. Accept puts the word on the name
        list and replaces it in every transcript now; "Not a name" puts it on the project word list, so it is not suggested again.{' '}
        {dictionary ? `Using Windows' ${dictionary} dictionary.` : "Windows has no dictionary for this language, so capital letters alone decide."}
      </p>
      <div className="row">
        <Button onPress={find} isDisabled={busy}>
          {busy ? 'Looking…' : 'Look again'}
        </Button>
        {list && <span role="status">{list.length === 0 ? 'No possible names left to review.' : `${list.length} possible name${list.length === 1 ? '' : 's'}.`}</span>}
      </div>
      {list && list.length > 0 && (
        <ul className="review-list">
          {list.map((s) => {
            const r = reps[s.word] ?? { replacement: '', kind: 'person' as NameKind };
            const drop = () => setList((l) => (l ?? []).filter((x) => x.word !== s.word));
            return (
              <li key={s.word} className="review-item">
                <div className="review-head">
                  <strong>{s.word}</strong>
                  <span className="hint">
                    {s.likelihood === 'likely' ? 'likely a name' : 'possibly a name'} · {s.count} time{s.count === 1 ? '' : 's'} in {s.files.join(', ')}
                  </span>
                </div>
                {s.examples.map(([label, at, before, after], i) => (
                  <p key={i} className="context">
                    <span className="hint">
                      {label} {hms(at)}:{' '}
                    </span>
                    {before}
                    <mark>{s.word}</mark>
                    {after}
                  </p>
                ))}
                <div className="row">
                  <input aria-label={`Replace ${s.word} with`} placeholder="[Person n], P08…" value={r.replacement} onChange={(e) => setReps({ ...reps, [s.word]: { ...r, replacement: e.target.value } })} />
                  <select aria-label={`Kind of ${s.word}`} value={r.kind} onChange={(e) => setReps({ ...reps, [s.word]: { ...r, kind: e.target.value as NameKind } })}>
                    {Object.entries(KIND_WORDS).map(([k, v]) => (
                      <option key={k} value={k}>
                        {v}
                      </option>
                    ))}
                  </select>
                  <Button
                    className="primary"
                    onPress={async () => {
                      try {
                        const done = await api.suggestionAccept(s.word, r.replacement, r.kind);
                        props.onNotice(`${s.word} is on the name list: ${counted(done)}.`);
                        drop();
                        props.onChanged();
                      } catch (e) {
                        props.onNotice(message(e));
                      }
                    }}
                  >
                    Accept
                  </Button>
                  <Button
                    onPress={async () => {
                      try {
                        await api.suggestionSkip(s.word);
                        drop();
                        props.onChanged();
                      } catch (e) {
                        props.onNotice(message(e));
                      }
                    }}
                  >
                    Not a name
                  </Button>
                </div>
              </li>
            );
          })}
        </ul>
      )}
      <section className="word-list" aria-labelledby="word-list-heading">
        <h3 id="word-list-heading">The project word list</h3>
        <p className="hint">Pseudonyms, field terms and words that are not names, one per line. They are never suggested as names.</p>
        <TextField value={words} onChange={setWords} aria-labelledby="word-list-heading">
          <TextArea rows={5} className="roster" />
        </TextField>
        <div className="row">
          <Button
            onPress={async () => {
              try {
                await api.setWordList(words.split('\n'));
                props.onChanged();
                props.onNotice('The word list is saved.');
              } catch (e) {
                props.onNotice(message(e));
              }
            }}
          >
            Save the word list
          </Button>
        </div>
      </section>
    </div>
  );
}

// ---------------------------------------------------------------------------------------------------------
// The final check and the log
// ---------------------------------------------------------------------------------------------------------

function CheckTab(props: { onNotice: (s: string) => void; onChanged: () => void }) {
  const [checks, setChecks] = useState<FileCheck[] | null>(null);
  const run = useCallback(async () => {
    try {
      setChecks(await api.finalCheck());
    } catch (e) {
      props.onNotice(message(e));
    }
  }, []);
  useEffect(() => {
    run();
  }, [run]);
  const done = checks?.filter((c) => c.transcribed) ?? [];
  const passing = done.filter((c) => c.leftovers.length === 0).length;
  return (
    <div className="deid-tab">
      <p className="hint">
        Looks for every name and spelling on the list in everything an export carries: the words, speaker names and attributes, and each file's
        label, interviewer and note. An export marked de-identified is refused while any is left.
      </p>
      <div className="row">
        <Button onPress={run}>Check again</Button>
        {checks && (
          <span role="status">
            {done.length === 0 ? 'No transcripts yet.' : passing === done.length ? `All ${done.length} transcribed file${done.length === 1 ? ' passes' : 's pass'}: no name from the list is left.` : `${passing} of ${done.length} transcribed files pass.`}
          </span>
        )}
      </div>
      <ul className="review-list">
        {done.map((c) => (
          <li key={c.file} className={c.leftovers.length ? 'review-item fails' : 'review-item passes'}>
            <div className="review-head">
              <strong>{c.label}</strong>
              <span className="hint">{REVIEW_WORDS[c.review]}</span>
            </div>
            {c.leftovers.length === 0 ? (
              <p>Passes: no name from the list.</p>
            ) : (
              <p>
                {c.leftovers.length} left:{' '}
                {c.leftovers
                  .slice(0, 12)
                  .map((l) => `${l.text} (${l.place})`)
                  .join(', ')}
                {c.leftovers.length > 12 ? ` and ${c.leftovers.length - 12} more` : ''}. Replace names on the Names tab, or fix the file's details.
              </p>
            )}
            {c.snapshots_with_names > 0 && (
              <p className="hint">
                {c.snapshots_with_names} snapshot{c.snapshots_with_names === 1 ? '' : 's'} of this transcript still hold names from before; snapshots stay in the project folder and are never
                exported.
              </p>
            )}
            {c.leftovers.length === 0 && c.review !== 'de_identified' && c.review !== 'final' && (
              <div className="row">
                <Button
                  onPress={async () => {
                    try {
                      await api.markDeidentified(c.file);
                      props.onChanged();
                      await run();
                    } catch (e) {
                      props.onNotice(message(e));
                    }
                  }}
                >
                  Mark as de-identified
                </Button>
              </div>
            )}
          </li>
        ))}
      </ul>
    </div>
  );
}

function LogTab(props: { onNotice: (s: string) => void }) {
  const [log, setLog] = useState<LogEntry[] | null>(null);
  useEffect(() => {
    api.deidLog().then(setLog).catch((e) => props.onNotice(message(e)));
  }, []);
  if (!log) return null;
  return (
    <div className="deid-tab">
      <p className="hint">Every change, newest first. The log is in the key file and never exported.</p>
      {log.length === 0 ? (
        <p>No changes yet.</p>
      ) : (
        <table className="log-table">
          <thead>
            <tr>
              <th scope="col">When</th>
              <th scope="col">File</th>
              <th scope="col">Where</th>
              <th scope="col">Original</th>
              <th scope="col">Replaced with</th>
              <th scope="col">Why</th>
              <th scope="col">Accepted by</th>
            </tr>
          </thead>
          <tbody>
            {log.slice(0, 1000).map((l, i) => (
              <tr key={i}>
                <td>{new Date(l.when).toLocaleString()}</td>
                <td>{l.label}</td>
                <td>{l.place}</td>
                <td>{l.original}</td>
                <td>{l.replacement}</td>
                <td>{l.reason}</td>
                <td>{l.by}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      {log.length > 1000 && <p className="hint">Showing the newest 1,000 of {log.length}.</p>}
    </div>
  );
}

// ---------------------------------------------------------------------------------------------------------
// The view
// ---------------------------------------------------------------------------------------------------------

export function DeidView(props: { project: ProjectView; onClose: () => void; onNotice: (s: string) => void; onChanged: () => void }) {
  const [key, setKey] = useState<KeyState | null>(null);
  const [tab, setTab] = useState<string>('names');
  // Each tab reloads when it is shown, so a change made on one is seen on the next.
  const [stamp, setStamp] = useState(0);
  useEffect(() => {
    api.keyState().then(setKey).catch((e) => props.onNotice(message(e)));
  }, []);
  const changed = () => {
    props.onChanged();
    api.keyState().then(setKey);
  };
  return (
    <main className="deid">
      <div className="editor-bar">
        <Button onPress={props.onClose}>← Recordings</Button>
        <h2 className="editor-title">De-identify {props.project.name}</h2>
        {key?.state === 'ready' && (
          <span className="hint">
            Key file: <span className="path">{key.path}</span> · {key.names} name{key.names === 1 ? '' : 's'} · {key.changes} change{key.changes === 1 ? '' : 's'} logged
          </span>
        )}
      </div>
      <div className="deid-body">
        {!key ? null : key.state !== 'ready' ? (
          <KeyChooser project={props.project} state={key} onState={setKey} onNotice={props.onNotice} />
        ) : (
          <>
            {key.inside_project && (
              <p className="warning" role="alert">
                The key file is inside the project folder, so anyone given the project folder would have the real names too. Keep it somewhere apart:
                save a new one elsewhere with "Start a new key file" after moving this one, or move the file and find it again.
              </p>
            )}
            <Tabs
              selectedKey={tab}
              onSelectionChange={(k) => {
                setTab(String(k));
                setStamp((s) => s + 1);
              }}
            >
              <TabList aria-label="De-identification steps" className="tabs">
                <Tab id="names">1. Names</Tab>
                <Tab id="patterns">2. Personal details</Tab>
                <Tab id="suggestions">3. Possible names</Tab>
                <Tab id="check">4. Final check</Tab>
                <Tab id="log">Log</Tab>
              </TabList>
              <TabPanel id="names">
                <NamesTab key={stamp} onNotice={props.onNotice} onChanged={changed} />
              </TabPanel>
              <TabPanel id="patterns">
                <PatternsTab key={stamp} onNotice={props.onNotice} onChanged={changed} />
              </TabPanel>
              <TabPanel id="suggestions">
                <SuggestionsTab key={stamp} project={props.project} onNotice={props.onNotice} onChanged={changed} />
              </TabPanel>
              <TabPanel id="check">
                <CheckTab key={stamp} onNotice={props.onNotice} onChanged={changed} />
              </TabPanel>
              <TabPanel id="log">
                <LogTab key={stamp} onNotice={props.onNotice} />
              </TabPanel>
            </Tabs>
          </>
        )}
      </div>
    </main>
  );
}
