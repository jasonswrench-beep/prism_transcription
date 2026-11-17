// The editor's dialogs: find and replace, the notation menu, snapshots, and the hotkey remap screen.
import { useEffect, useRef, useState } from 'react';
import { Button, Checkbox, Dialog, Heading, Input, Label, ListBox, ListBoxItem, Modal, ModalOverlay, Radio, RadioGroup, TextField } from 'react-aria-components';
import type { Snapshot } from '../api';
import { ACTION_LABELS, clashes, keyName, showKey, type Action } from './keys';
import type { FindOptions } from './words';

export function FindBar(props: {
  state: { query: string; options: FindOptions; at: number };
  count: number;
  onChange: (s: { query: string; options: FindOptions; at: number }) => void;
  onNext: (d: number) => void;
  onReplaceAll: (replace: string, wholeProject: boolean) => void;
  onClose: () => void;
}) {
  const [replace, setReplace] = useState('');
  const [scope, setScope] = useState('this');
  const s = props.state;
  return (
    <div
      className="find-bar"
      role="search"
      onKeyDown={(e) => {
        if (e.key === 'Escape') props.onClose();
        if (e.key === 'Enter') {
          e.preventDefault();
          props.onNext(e.shiftKey ? -1 : 1);
        }
      }}
    >
      <TextField value={s.query} onChange={(q) => props.onChange({ ...s, query: q, at: 0 })} autoFocus>
        <Label>Find</Label>
        <Input />
      </TextField>
      <TextField value={replace} onChange={setReplace}>
        <Label>Replace with</Label>
        <Input />
      </TextField>
      <Checkbox isSelected={s.options.whole_word} onChange={(v) => props.onChange({ ...s, options: { ...s.options, whole_word: v } })}>
        <span className="box" aria-hidden="true" /> Whole word
      </Checkbox>
      <Checkbox isSelected={s.options.match_case} onChange={(v) => props.onChange({ ...s, options: { ...s.options, match_case: v } })}>
        <span className="box" aria-hidden="true" /> Match case
      </Checkbox>
      <RadioGroup value={scope} onChange={setScope} orientation="horizontal" aria-label="Where to replace">
        <Radio value="this">This transcript</Radio>
        <Radio value="project">Whole project</Radio>
      </RadioGroup>
      <span role="status" className="find-count">
        {s.query ? (props.count ? `${(s.at % props.count) + 1} of ${props.count}` : 'Not found') : ''}
      </span>
      <Button onPress={() => props.onNext(-1)} isDisabled={!props.count}>Previous</Button>
      <Button onPress={() => props.onNext(1)} isDisabled={!props.count}>Next</Button>
      <Button className="primary" isDisabled={!s.query} onPress={() => props.onReplaceAll(replace, scope === 'project')}>
        Replace all
      </Button>
      <Button className="quiet" onPress={props.onClose}>Close</Button>
    </div>
  );
}

export function NotationMenu(props: { isOpen: boolean; entries: string[]; onClose: () => void; onPick: (entry: string) => void; onSaveList: (list: string[]) => void }) {
  const [editing, setEditing] = useState(false);
  const [text, setText] = useState('');
  useEffect(() => {
    if (props.isOpen) {
      setEditing(false);
      setText(props.entries.join('\n'));
    }
  }, [props.isOpen, props.entries]);
  return (
    <ModalOverlay isOpen={props.isOpen} onOpenChange={(o) => !o && props.onClose()} isDismissable>
      <Modal>
        <Dialog aria-label="Insert notation">
          <Heading slot="title">Insert notation</Heading>
          {!editing ? (
            <>
              <p className="hint">Arrow keys and Enter, or click. "hh:mm:ss" becomes the playback time; "…" is selected for you to type over.</p>
              <ListBox aria-label="Notation" selectionMode="single" autoFocus="first" onAction={(k) => props.onPick(String(k))} className="notation-list">
                {props.entries.map((e) => (
                  <ListBoxItem key={e} id={e} textValue={e}>
                    {e}
                  </ListBoxItem>
                ))}
              </ListBox>
              <div className="row end">
                <Button onPress={() => setEditing(true)}>Edit this list…</Button>
                <Button onPress={props.onClose}>Cancel</Button>
              </div>
            </>
          ) : (
            <>
              <p className="hint">One entry per line, each in square brackets. This list belongs to the project.</p>
              <TextField value={text} onChange={setText} aria-label="Notation entries">
                <textarea className="react-aria-Input notation-edit" rows={8} value={text} onChange={(e) => setText(e.target.value)} />
              </TextField>
              <div className="row end">
                <Button onPress={() => setEditing(false)}>Back</Button>
                <Button
                  className="primary"
                  onPress={() => {
                    props.onSaveList(text.split('\n').map((l) => l.trim()).filter(Boolean));
                    setEditing(false);
                  }}
                >
                  Save list
                </Button>
              </div>
            </>
          )}
        </Dialog>
      </Modal>
    </ModalOverlay>
  );
}

export function SnapshotsDialog(props: { isOpen: boolean; snapshots: Snapshot[]; onClose: () => void; onSave: (name: string) => void; onRestore: (index: number) => void }) {
  const [name, setName] = useState('');
  useEffect(() => {
    if (props.isOpen) setName('');
  }, [props.isOpen]);
  const words = (s: Snapshot) => s.turns.reduce((n, t) => n + t.words.length, 0);
  return (
    <ModalOverlay isOpen={props.isOpen} onOpenChange={(o) => !o && props.onClose()} isDismissable>
      <Modal>
        <Dialog>
          <Heading slot="title">Snapshots</Heading>
          <p className="hint">A snapshot keeps a copy of the transcript as it is now. Version 0 is what the recognizer heard and is always kept.</p>
          <ul className="snapshot-list">
            {props.snapshots.map((s, i) => (
              <li key={i}>
                <span>
                  <strong>{s.name}</strong> · {new Date(s.created).toLocaleString()} · {words(s)} words
                </span>
                <Button onPress={() => props.onRestore(i)}>Restore</Button>
              </li>
            ))}
          </ul>
          <TextField value={name} onChange={setName} autoFocus>
            <Label>Name for a new snapshot</Label>
            <Input placeholder="After second listen" />
          </TextField>
          <div className="row end">
            <Button onPress={props.onClose}>Close</Button>
            <Button className="primary" isDisabled={!name.trim()} onPress={() => { props.onSave(name.trim()); setName(''); }}>
              Save snapshot
            </Button>
          </div>
        </Dialog>
      </Modal>
    </ModalOverlay>
  );
}

export function HotkeysDialog(props: { isOpen: boolean; keys: Record<string, string>; rewind: number; onClose: () => void; onSave: (keys: Record<string, string>, rewind: number) => void }) {
  const [keys, setKeys] = useState<Record<string, string>>(props.keys);
  const [rewind, setRewind] = useState(String(props.rewind));
  const [listening, setListening] = useState<Action | null>(null);
  const listenRef = useRef<Action | null>(null);
  listenRef.current = listening;
  useEffect(() => {
    if (props.isOpen) {
      setKeys(props.keys);
      setRewind(String(props.rewind));
      setListening(null);
    }
  }, [props.isOpen, props.keys, props.rewind]);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const a = listenRef.current;
      if (!a) return;
      if (e.key === 'Escape') {
        setListening(null);
        e.preventDefault();
        e.stopPropagation();
        return;
      }
      const name = keyName(e);
      if (!name) return;
      e.preventDefault();
      e.stopPropagation();
      setKeys((k) => ({ ...k, [a]: name }));
      setListening(null);
    };
    window.addEventListener('keydown', onKey, true);
    return () => window.removeEventListener('keydown', onKey, true);
  }, []);
  const clash = clashes(keys);
  const r = Number(rewind);
  return (
    <ModalOverlay isOpen={props.isOpen} onOpenChange={(o) => !o && !listening && props.onClose()} isDismissable={!listening}>
      <Modal>
        <Dialog>
          <Heading slot="title">Keys</Heading>
          <p className="hint">
            Choose Change, then press the new key. A USB foot pedal that works as a keyboard can be set up here: press the pedal. Escape cancels.
          </p>
          <table className="keys-table">
            <tbody>
              {(Object.keys(ACTION_LABELS) as Action[]).map((a) => (
                <tr key={a}>
                  <th scope="row">{ACTION_LABELS[a]}</th>
                  <td className={clash.includes(keys[a]) ? 'clash' : undefined}>{listening === a ? 'Press a key…' : showKey(keys[a] ?? '')}</td>
                  <td>
                    <Button onPress={() => setListening(a)} aria-label={`Change the key for ${ACTION_LABELS[a]}`}>
                      Change
                    </Button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
          {clash.length > 0 && <p className="error">Two actions share {clash.map(showKey).join(', ')}; give one of them another key.</p>}
          <TextField value={rewind} onChange={setRewind}>
            <Label>Rewind on resume (seconds)</Label>
            <Input inputMode="decimal" />
          </TextField>
          <div className="row end">
            <Button onPress={() => import('../api').then(({ api }) => api.defaultHotkeys().then(setKeys))}>Reset to defaults</Button>
            <Button onPress={props.onClose}>Cancel</Button>
            <Button className="primary" isDisabled={clash.length > 0 || !(r >= 0 && r <= 10)} onPress={() => props.onSave(keys, r)}>
              Save
            </Button>
          </div>
        </Dialog>
      </Modal>
    </ModalOverlay>
  );
}
