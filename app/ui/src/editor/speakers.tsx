// The speakers panel (T3): rename a speaker once and every turn follows; role and attributes (they travel to PrismQDA
// as case attributes); merge two speakers; add one; remove one who has no turns left; and find the speakers again
// with another expected count, keeping every word as it is.
import { useEffect, useState } from 'react';
import { Button, Dialog, Group, Heading, Input, Label, Modal, ModalOverlay, NumberField, TextField } from 'react-aria-components';
import type { Role, Speaker, SpeakerStats } from '../api';

export const ROLE_WORDS: Record<Role, string> = {
  interviewer: 'Interviewer',
  moderator: 'Moderator',
  participant: 'Participant',
  other: 'Other',
};

/** Attributes researchers usually record; any other name can be typed. */
const SUGGESTED = ['pseudonym', 'age band', 'site', 'gender', 'group'];

export function speakerName(s: Speaker): string {
  return s.name?.trim() || s.label;
}

function minutes(s: number): string {
  return s < 60 ? `${Math.round(s)} s` : `${(s / 60).toFixed(1)} min`;
}

function SpeakerRow(props: {
  speaker: Speaker;
  n: number;
  stats: SpeakerStats | undefined;
  others: Speaker[];
  keyFor: (n: number) => string;
  onChange: (s: Speaker) => void;
  onMerge: (into: string) => void;
  onRemove: () => void;
}) {
  const s = props.speaker;
  const [name, setName] = useState(s.name ?? '');
  const [attrs, setAttrs] = useState<[string, string][]>(Object.entries(s.attributes ?? {}));
  const [mergeInto, setMergeInto] = useState('');
  useEffect(() => setName(s.name ?? ''), [s.name]);
  // Rows being filled in (a name with no value yet) stay on screen; only complete ones are saved.
  const complete = (list: [string, string][]) => Object.fromEntries(list.map(([k, v]) => [k.trim(), v.trim()] as [string, string]).filter(([k, v]) => k && v));
  useEffect(() => {
    setAttrs((local) => (JSON.stringify(complete(local)) === JSON.stringify(s.attributes ?? {}) ? local : Object.entries(s.attributes ?? {})));
  }, [s.attributes]);
  const saveAttrs = (list: [string, string][]) => {
    const clean = complete(list);
    if (JSON.stringify(clean) !== JSON.stringify(s.attributes ?? {})) props.onChange({ ...s, attributes: clean });
  };
  const used = props.stats?.turns ?? 0;
  const colour = ((props.n - 1) % 8) + 1;
  return (
    <section className="speaker-card" style={{ borderLeftColor: `var(--s${colour})` }} aria-label={`${speakerName(s)} (${s.label})`}>
      <div className="speaker-card-head">
        <strong>{s.label}</strong>
        <span className="hint">
          {props.n <= 9 ? `${props.keyFor(props.n)} gives a turn to this speaker · ` : ''}
          {used} turn{used === 1 ? '' : 's'}, {props.stats?.words ?? 0} words, {minutes(props.stats?.seconds ?? 0)}
        </span>
      </div>
      <div className="speaker-fields">
        <TextField value={name} onChange={setName} onBlur={() => name !== (s.name ?? '') && props.onChange({ ...s, name: name.trim() || null })}>
          <Label>Name shown in the transcript</Label>
          <Input placeholder={s.label} onKeyDown={(e) => e.key === 'Enter' && (e.target as HTMLInputElement).blur()} />
        </TextField>
        <label className="field">
          <span>Role</span>
          <select value={s.role} onChange={(e) => props.onChange({ ...s, role: e.target.value as Role })}>
            {Object.entries(ROLE_WORDS).map(([k, v]) => (
              <option key={k} value={k}>
                {v}
              </option>
            ))}
          </select>
        </label>
      </div>
      <fieldset className="attributes">
        <legend>Attributes (sent to PrismQDA as case attributes)</legend>
        {attrs.map(([k, v], i) => (
          <Group key={i} className="attribute-row" aria-label={`Attribute ${i + 1}`}>
            <input
              aria-label="Attribute name"
              list="attribute-names"
              value={k}
              onChange={(e) => setAttrs(attrs.map((a, j) => (j === i ? [e.target.value, a[1]] : a)))}
              onBlur={() => saveAttrs(attrs)}
            />
            <input aria-label={`Value of ${k || 'the attribute'}`} value={v} onChange={(e) => setAttrs(attrs.map((a, j) => (j === i ? [a[0], e.target.value] : a)))} onBlur={() => saveAttrs(attrs)} />
            <Button
              className="quiet"
              aria-label={`Remove the attribute ${k}`}
              onPress={() => {
                const next = attrs.filter((_, j) => j !== i);
                setAttrs(next);
                saveAttrs(next);
              }}
            >
              Remove
            </Button>
          </Group>
        ))}
        <Button className="quiet" onPress={() => setAttrs([...attrs, ['', '']])}>
          Add an attribute
        </Button>
      </fieldset>
      <div className="row">
        {props.others.length > 0 && (
          <>
            <label className="field inline">
              <span>Merge into</span>
              <select value={mergeInto} onChange={(e) => setMergeInto(e.target.value)} aria-label={`Merge ${speakerName(s)} into another speaker`}>
                <option value="">Choose a speaker…</option>
                {props.others.map((o) => (
                  <option key={o.id} value={o.id}>
                    {speakerName(o)}
                  </option>
                ))}
              </select>
            </label>
            <Button isDisabled={!mergeInto} onPress={() => props.onMerge(mergeInto)}>
              Merge
            </Button>
          </>
        )}
        {used === 0 && (
          <Button className="quiet" onPress={props.onRemove}>
            Remove this speaker
          </Button>
        )}
      </div>
    </section>
  );
}

export function SpeakersDialog(props: {
  isOpen: boolean;
  speakers: Speaker[];
  stats: SpeakerStats[];
  /** Finding speakers again needs one mix; a file transcribed track by track already knows its speakers. */
  canFindAgain: boolean;
  expected: number | null;
  keyFor: (n: number) => string;
  onClose: () => void;
  onChange: (s: Speaker) => void;
  onMerge: (from: string, into: string) => void;
  onAdd: () => void;
  onRemove: (id: string) => void;
  onFindAgain: (expected: number | null) => Promise<void>;
}) {
  const [count, setCount] = useState<number>(props.expected ?? NaN);
  const [finding, setFinding] = useState(false);
  useEffect(() => {
    if (props.isOpen) setCount(props.expected ?? NaN);
  }, [props.isOpen, props.expected]);
  return (
    <ModalOverlay isOpen={props.isOpen} onOpenChange={(o) => !o && !finding && props.onClose()} isDismissable={!finding}>
      <Modal className="react-aria-Modal wide">
        <Dialog>
          <Heading slot="title">Speakers</Heading>
          <p className="hint">
            A new name shows on every turn of that speaker. In a turn, {props.keyFor(1)} to {props.keyFor(9)} give the turn, or the words selected in it, to speakers 1 to 9; Enter splits a turn at the cursor and Backspace at the start of a turn joins it to the one above. Everything here can be undone with Ctrl+Z.
          </p>
          <datalist id="attribute-names">
            {SUGGESTED.map((a) => (
              <option key={a} value={a} />
            ))}
          </datalist>
          <div className="speaker-cards">
            {props.speakers.map((s, i) => (
              <SpeakerRow
                key={s.id}
                speaker={s}
                n={i + 1}
                stats={props.stats.find((x) => x.id === s.id)}
                others={props.speakers.filter((o) => o.id !== s.id)}
                keyFor={props.keyFor}
                onChange={props.onChange}
                onMerge={(into) => props.onMerge(s.id, into)}
                onRemove={() => props.onRemove(s.id)}
              />
            ))}
          </div>
          <div className="row">
            <Button onPress={props.onAdd}>Add a speaker</Button>
          </div>
          {props.canFindAgain && (
            <section className="find-again" aria-labelledby="find-again-heading">
              <h3 id="find-again-heading">Find the speakers again</h3>
              <p className="hint">
                Listens for who is speaking once more, with the number of speakers you expect, and gives every word to the speaker heard at its time. Your edits to the words are kept; names, roles and attributes start again. Takes a minute or two for an hour of recording.
              </p>
              <div className="row">
                <NumberField value={count} onChange={setCount} minValue={1} maxValue={20} aria-label="Expected number of speakers">
                  <Label>Expected number of speakers (empty to let it decide)</Label>
                  <Input />
                </NumberField>
                <Button
                  isDisabled={finding}
                  onPress={async () => {
                    setFinding(true);
                    try {
                      await props.onFindAgain(Number.isFinite(count) ? count : null);
                    } finally {
                      setFinding(false);
                    }
                  }}
                >
                  {finding ? 'Finding speakers…' : 'Find speakers again'}
                </Button>
              </div>
            </section>
          )}
          <div className="row end">
            <Button className="primary" isDisabled={finding} onPress={props.onClose}>
              Done
            </Button>
          </div>
        </Dialog>
      </Modal>
    </ModalOverlay>
  );
}
