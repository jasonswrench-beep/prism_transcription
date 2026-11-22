// Help (F1 or the Help button): getting started, the keys as they are set now, privacy in brief, and the version.
// Everything is inside the app, so Help works offline. It can be printed, as PrismQDA's shortcut list can.
import { useEffect, useState } from 'react';
import { Button, Dialog, Heading, Modal, ModalOverlay } from 'react-aria-components';
import { getVersion } from '@tauri-apps/api/app';
import { api } from './api';
import { ACTION_LABELS, showKey, type Action } from './editor/keys';
import { UpdatesCard } from './updates';

const STEPS: [string, string][] = [
  ['Start a project', 'New project makes one folder for a study. The recordings stay where they are unless you choose to copy them in.'],
  ['Get the speech models', 'The first time, Prism Transcribe offers to download the light pack (about 515 MB to download, about 700 MB once installed), or to import it from an offline zip. The heavy pack, for about 99 languages, is in Settings.'],
  ['Import recordings', 'Import… takes audio and video. Give each a label, a date, its language and the number of speakers you expect, if you know them. Details… changes these later; its Advanced settings choose the model pack, or one microphone per person.'],
  ['Transcribe', 'Transcribe puts a recording in the queue and runs it. Queue all untranscribed adds every recording that has no transcript yet; the queue works on one at a time, in the order you set by dragging. You can pause, resume, hold a file or cancel; the queue carries on after a restart.'],
  ['Check and correct', 'Edit opens the transcript beside the recording. Click a word to play from it; type to correct it. The keys below play, pause and move without the mouse. Mark how far you have checked, and set the review status.'],
  ['Name the speakers', 'Speakers (Ctrl+Shift+P) gives each speaker a name, a role and attributes, merges speakers, and finds them again with another expected count. Alt+1 to Alt+9 give a turn, or the words selected in it, to a speaker.'],
  ['De-identify', 'De-identify keeps a key file apart from the project: the real names and their replacements, and a log of every change. Replace the names on the list, review personal details and possible names, then run the final check.'],
  ['Export', 'Export… writes Word, plain text, captions, a table, a REFI-QDA project for PrismQDA (with click-to-play) or the Prism transcript package. Export all… puts every transcript into one table or package. A de-identified export is refused while a name from the list is left.'],
];

export function HelpDialog(props: { isOpen: boolean; onClose: () => void }) {
  const [keys, setKeys] = useState<Record<string, string>>({});
  const [version, setVersion] = useState('');
  useEffect(() => {
    if (!props.isOpen) return;
    getVersion().then(setVersion).catch(() => setVersion(''));
    Promise.all([api.defaultHotkeys(), api.settings()])
      .then(([defaults, s]) => {
        const mine = ((s as unknown as { hotkeys?: Record<string, string> }).hotkeys ?? {}) as Record<string, string>;
        const merged = { ...defaults };
        for (const [a, k] of Object.entries(mine)) if (a in merged && k.trim()) merged[a] = k;
        setKeys(merged);
      })
      .catch(() => setKeys({}));
  }, [props.isOpen]);
  const editorKeys = (Object.keys(ACTION_LABELS) as Action[]).filter((a) => keys[a]);
  return (
    <ModalOverlay isOpen={props.isOpen} onOpenChange={(o) => !o && props.onClose()} isDismissable>
      <Modal className="react-aria-Modal wide help-modal">
        <Dialog aria-labelledby="help-title">
          <Heading slot="title" id="help-title">
            Help
          </Heading>
          <section aria-labelledby="help-start" className="help-section">
            <h3 id="help-start">Getting started</h3>
            <ol className="help-steps">
              {STEPS.map(([t, d]) => (
                <li key={t}>
                  <strong>{t}.</strong> {d}
                </li>
              ))}
            </ol>
          </section>
          <section aria-labelledby="help-keys" className="help-section">
            <h3 id="help-keys">Keys in the editor</h3>
            <p className="hint">As they are set now. Change any of them with the Keys button in the editor.</p>
            <table className="keys-table">
              <caption className="visually-hidden">Keys in the editor</caption>
              <thead>
                <tr>
                  <th scope="col">Keys</th>
                  <th scope="col">What they do</th>
                </tr>
              </thead>
              <tbody>
                {editorKeys.map((a) => (
                  <tr key={a}>
                    <th scope="row">
                      <kbd>{showKey(keys[a])}</kbd>
                    </th>
                    <td>{ACTION_LABELS[a]}</td>
                  </tr>
                ))}
                <tr>
                  <th scope="row">
                    <kbd>Enter</kbd>
                  </th>
                  <td>Split the turn at the cursor</td>
                </tr>
                <tr>
                  <th scope="row">
                    <kbd>Backspace</kbd>
                  </th>
                  <td>At the start of a turn: join it to the one above</td>
                </tr>
                <tr>
                  <th scope="row">
                    <kbd>Ctrl+Z</kbd>, <kbd>Ctrl+Y</kbd>
                  </th>
                  <td>Undo, redo</td>
                </tr>
                <tr>
                  <th scope="row">
                    <kbd>Ctrl+S</kbd>
                  </th>
                  <td>Save now (it also saves every 30 seconds)</td>
                </tr>
                <tr>
                  <th scope="row">
                    <kbd>F1</kbd>
                  </th>
                  <td>This help</td>
                </tr>
              </tbody>
            </table>
          </section>
          <section aria-labelledby="help-privacy" className="help-section">
            <h3 id="help-privacy">Privacy in brief</h3>
            <p>
              Recordings, transcripts and names stay on this computer. The speech models run here. The only network calls are the ones you start:
              downloading models, and checking for updates. The Offline switch in Settings blocks both. Privacy has optional password protection and
              a data handling statement for an ethics protocol.
            </p>
          </section>
          <section aria-labelledby="help-about" className="help-section">
            <h3 id="help-about">About</h3>
            <p>
              Prism Transcribe {version || ''}, free software under the Apache License 2.0. The models' licences and credits are in Settings. Questions and
              problems: the project's page, github.com/jasonswrench-beep/prism_transcription.
            </p>
          </section>
          <div className="no-print">
            <UpdatesCard />
          </div>
          <div className="row end no-print">
            <Button onPress={() => window.print()}>Print</Button>
            <Button className="primary" onPress={props.onClose}>
              Close
            </Button>
          </div>
        </Dialog>
      </Modal>
    </ModalOverlay>
  );
}
