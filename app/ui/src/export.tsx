// Export (T7): Word, plain text, captions (SubRip, WebVTT), the table (CSV, Excel), the PrismQDA package (.prismtx)
// and a REFI-QDA project (.qdpx). The table and the two packages can hold every transcribed recording at once. Every
// export can be clean or verbatim, de-identified or not; one not yet de-identified shows a warning naming the files.
import { useEffect, useState } from 'react';
import { Button, Checkbox, Dialog, Heading, Label, Modal, ModalOverlay, Radio, RadioGroup } from 'react-aria-components';
import { save } from '@tauri-apps/plugin-dialog';
import { api, type ExportCheck, type ExportOptions, type MediaFile } from './api';

export type ExportKind = 'docx' | 'txt' | 'srt' | 'vtt' | 'csv' | 'xlsx' | 'prismtx' | 'qdpx';

const KINDS: { kind: ExportKind; label: string; filter: string; many: boolean }[] = [
  { kind: 'docx', label: 'Word (.docx)', filter: 'Word document', many: false },
  { kind: 'txt', label: 'Plain text (.txt)', filter: 'Plain text', many: false },
  { kind: 'srt', label: 'Captions, SubRip (.srt)', filter: 'SubRip captions', many: false },
  { kind: 'vtt', label: 'Captions, WebVTT (.vtt)', filter: 'WebVTT captions', many: false },
  { kind: 'csv', label: 'Table, CSV (.csv): one row per turn', filter: 'CSV table', many: true },
  { kind: 'xlsx', label: 'Table, Excel (.xlsx): one row per turn', filter: 'Excel workbook', many: true },
  { kind: 'prismtx', label: 'PrismQDA package (.prismtx)', filter: 'Prism transcript package', many: true },
  { kind: 'qdpx', label: 'REFI-QDA project (.qdpx): opens in PrismQDA today, and other QDA programs', filter: 'REFI-QDA project', many: true },
];

function message(e: unknown): string {
  return typeof e === 'string' ? e : e instanceof Error ? e.message : String(e);
}

/** `files` is one recording (its Export button) or every transcribed one (Export all). */
export function ExportDialog(props: { files: MediaFile[] | null; projectName: string; onClose: () => void; onNotice: (s: string) => void; onExported: (kind: ExportKind) => void }) {
  const [kind, setKind] = useState<ExportKind>('docx');
  const [stamps, setStamps] = useState('per_turn');
  const [clean, setClean] = useState(false);
  const [lines, setLines] = useState(false);
  const [names, setNames] = useState(true);
  const [check, setCheck] = useState<ExportCheck | null>(null);
  const [deid, setDeid] = useState(false);
  const [busy, setBusy] = useState(false);
  const files = props.files ?? [];
  const many = files.length > 1;
  useEffect(() => {
    setCheck(null);
    if (!props.files) return;
    if (props.files.length > 1) setKind((k) => (KINDS.find((x) => x.kind === k)?.many ? k : 'xlsx'));
    api
      .exportCheck(props.files.map((f) => f.id))
      .then((c) => {
        setCheck(c);
        setDeid(c.key.state === 'ready' && c.files.every((f) => f.deidentified));
      })
      .catch(() => setCheck(null));
  }, [props.files]);
  const keyReady = check?.key.state === 'ready';
  const notDeid = check?.files.filter((f) => !f.deidentified) ?? [];
  const left = check?.files.flatMap((f) => (f.check?.leftovers ?? []).map((l) => ({ ...l, label: f.label }))) ?? [];
  const k = KINDS.find((x) => x.kind === kind)!;
  const isText = kind === 'docx' || kind === 'txt';
  const isCaptions = kind === 'srt' || kind === 'vtt';
  const run = async () => {
    const base = many ? props.projectName : files[0].label;
    const path = await save({ title: `Export ${many ? 'all transcripts' : files[0].label}`, defaultPath: `${base}.${kind}`, filters: [{ name: k.filter, extensions: [kind] }] });
    if (!path) return;
    const options: ExportOptions = {
      timestamps: stamps === 'every' ? { mode: 'every', seconds: 30 } : stamps === 'none' ? { mode: 'none' } : { mode: 'per_turn' },
      line_numbers: kind === 'docx' && lines,
      clean,
      speaker_names: names,
    };
    setBusy(true);
    try {
      await api.exportFile(
        files.map((f) => f.id),
        path,
        options,
        deid,
      );
      props.onNotice(`Exported ${many ? `${files.length} transcripts` : files[0].label} to ${path}${deid ? '. Checked: no name from the list is in it' : ''}.`);
      props.onExported(kind);
      props.onClose();
    } catch (e) {
      props.onNotice(message(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <ModalOverlay isOpen={!!props.files} onOpenChange={(o) => !o && !busy && props.onClose()} isDismissable={!busy}>
      <Modal className="react-aria-Modal wide">
        <Dialog>
          <Heading slot="title">{many ? `Export all ${files.length} transcripts` : `Export ${files[0]?.label ?? ''}`}</Heading>
          <RadioGroup value={kind} onChange={(v) => setKind(v as ExportKind)}>
            <Label>Format</Label>
            {KINDS.map((x) => (
              <Radio key={x.kind} value={x.kind} isDisabled={many && !x.many}>
                {x.label}
              </Radio>
            ))}
          </RadioGroup>
          {many && <p className="hint">Word, text and captions hold one recording each; use each recording's Export button for those.</p>}
          {kind === 'prismtx' && <p className="hint">The transcript with its speakers as cases, their attributes and a link to the recording, for PrismQDA. PrismQDA's reader for it is on PrismQDA's development list; until then, the REFI-QDA project opens there with click-to-play.</p>}
          {isText && (
            <RadioGroup value={stamps} onChange={setStamps}>
              <Label>Timestamps</Label>
              <Radio value="per_turn">At the start of every turn</Radio>
              <Radio value="every">Every 30 seconds</Radio>
              <Radio value="none">None</Radio>
            </RadioGroup>
          )}
          {(isText || isCaptions || kind === 'csv' || kind === 'xlsx') && (
            <RadioGroup value={clean ? 'clean' : 'verbatim'} onChange={(v) => setClean(v === 'clean')}>
              <Label>Text</Label>
              <Radio value="verbatim">Verbatim: every word as said</Radio>
              <Radio value="clean">Clean: fillers and repeated words left out</Radio>
            </RadioGroup>
          )}
          {kind === 'docx' && (
            <Checkbox isSelected={lines} onChange={setLines}>
              <span className="box" aria-hidden="true" /> Number the lines
            </Checkbox>
          )}
          {isCaptions && (
            <Checkbox isSelected={names} onChange={setNames}>
              <span className="box" aria-hidden="true" /> Show who is speaking
            </Checkbox>
          )}
          <Checkbox isSelected={deid} onChange={setDeid} isDisabled={!keyReady}>
            <span className="box" aria-hidden="true" /> De-identified: check that no name from the list is in it
          </Checkbox>
          {check && !keyReady && <p className="hint">There is no key file for this project yet, so there is no name list to check against. Start in De-identify.</p>}
          {notDeid.length > 0 && (
            <p className="warning" role="alert">
              {notDeid.map((f) => f.label).join(', ')} {notDeid.length === 1 ? 'has' : 'have'} not been marked de-identified. This export may contain real names.
            </p>
          )}
          {deid && left.length > 0 && (
            <p className="warning" role="alert">
              {left.length} name{left.length === 1 ? '' : 's'} from the list {left.length === 1 ? 'is' : 'are'} still there (
              {left
                .slice(0, 5)
                .map((l) => `${l.text}, ${l.label} ${l.place}`)
                .join('; ')}
              {left.length > 5 ? '…' : ''}). The de-identified export will be refused until they are replaced.
            </p>
          )}
          {deid && keyReady && left.length === 0 && check && <p className="hint">Passes the final check: no name from the list.</p>}
          <div className="row end">
            <Button onPress={props.onClose} isDisabled={busy}>
              Cancel
            </Button>
            <Button className="primary" isDisabled={(deid && left.length > 0) || busy || files.length === 0} onPress={run}>
              {busy ? 'Exporting…' : 'Choose where to save…'}
            </Button>
          </div>
        </Dialog>
      </Modal>
    </ModalOverlay>
  );
}
