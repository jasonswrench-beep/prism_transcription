// Privacy: unlocking a password-protected project, turning protection on, changing or removing the password, and
// the data handling statement (one page for an ethics protocol).
// Fields marked invalid use validationBehavior="aria": the default, native validation, would stop the form from
// being sent again after one wrong password.
import { useEffect, useState } from 'react';
import { Button, Checkbox, Dialog, Heading, Input, Label, Modal, ModalOverlay, TextField } from 'react-aria-components';
import { save } from '@tauri-apps/plugin-dialog';
import { api, type ProjectView, type Settings } from './api';

function message(e: unknown): string {
  return typeof e === 'string' ? e : e instanceof Error ? e.message : String(e);
}

/** Shown instead of the recordings while a protected project is locked. */
export function UnlockPanel(props: { project: ProjectView; onUnlocked: (p: ProjectView) => void }) {
  const [pw, setPw] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const submit = async () => {
    setBusy(true);
    setError(null);
    try {
      props.onUnlocked(await api.unlock(pw));
      setPw('');
    } catch (e) {
      setError(message(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <main className="empty">
      <h2>{props.project.name} is password protected</h2>
      <p>Its transcripts are sealed with a password. Enter it to work on them; they stay sealed on disk either way.</p>
      <form
        className="unlock"
        onSubmit={(e) => {
          e.preventDefault();
          submit();
        }}
      >
        <TextField type="password" value={pw} onChange={(v) => { setPw(v); setError(null); }} autoFocus isInvalid={!!error} validationBehavior="aria">
          <Label>Password</Label>
          <Input />
        </TextField>
        <Button type="submit" className="primary" isDisabled={!pw || busy}>
          {busy ? 'Unlocking…' : 'Unlock'}
        </Button>
      </form>
      {error && (
        <p className="error" role="alert">
          {error[0].toUpperCase() + error.slice(1)}.
        </p>
      )}
    </main>
  );
}

function NewPassword(props: { label: string; busy: boolean; onSet: (pw: string) => void; needsConsent: boolean }) {
  const [a, setA] = useState('');
  const [b, setB] = useState('');
  const [understood, setUnderstood] = useState(false);
  const short = a.length > 0 && a.length < 8;
  const differ = b.length > 0 && a !== b;
  const ok = a.length >= 8 && a === b && (understood || !props.needsConsent);
  return (
    <form
      className="new-password"
      onSubmit={(e) => {
        e.preventDefault();
        if (ok) props.onSet(a);
      }}
    >
      <TextField type="password" value={a} onChange={setA} isInvalid={short} validationBehavior="aria">
        <Label>New password (at least 8 characters)</Label>
        <Input />
      </TextField>
      <TextField type="password" value={b} onChange={setB} isInvalid={differ} validationBehavior="aria">
        <Label>The same password again</Label>
        <Input />
      </TextField>
      {differ && <p className="error">The two passwords are not the same.</p>}
      {props.needsConsent && (
        <Checkbox isSelected={understood} onChange={setUnderstood}>
          <span className="box" aria-hidden="true" /> I understand that if this password is lost, the transcripts cannot be opened by anyone
        </Checkbox>
      )}
      <div className="row">
        <Button type="submit" className="primary" isDisabled={!ok || props.busy}>
          {props.busy ? 'Sealing the transcripts…' : props.label}
        </Button>
      </div>
    </form>
  );
}

export function PrivacyDialog(props: { isOpen: boolean; project: ProjectView; settings: Settings; onClose: () => void; onProject: (p: ProjectView) => void; onNotice: (s: string) => void }) {
  const [statement, setStatement] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [changing, setChanging] = useState(false);
  useEffect(() => {
    if (props.isOpen) {
      setChanging(false);
      api.statementText().then(setStatement).catch((e) => props.onNotice(message(e)));
    }
  }, [props.isOpen, props.project.protected, props.settings.offline]);
  const setPassword = async (pw: string | null, done: string) => {
    setBusy(true);
    try {
      props.onProject(await api.setPassword(pw));
      setStatement(await api.statementText());
      setChanging(false);
      props.onNotice(done);
    } catch (e) {
      props.onNotice(message(e));
    } finally {
      setBusy(false);
    }
  };
  const saveStatement = async (kind: 'docx' | 'txt') => {
    const path = await save({
      title: 'Save the data handling statement',
      defaultPath: `${props.project.name} data handling.${kind}`,
      filters: [{ name: kind === 'docx' ? 'Word document' : 'Plain text', extensions: [kind] }],
    });
    if (!path) return;
    try {
      await api.statementExport(path);
      props.onNotice(`Saved the data handling statement to ${path}.`);
    } catch (e) {
      props.onNotice(message(e));
    }
  };
  return (
    <ModalOverlay isOpen={props.isOpen} onOpenChange={(o) => !o && !busy && props.onClose()} isDismissable={!busy}>
      <Modal className="react-aria-Modal wide">
        <Dialog>
          <Heading slot="title">Privacy</Heading>
          <section aria-labelledby="pw-heading" className="privacy-section">
            <h3 id="pw-heading">Password protection</h3>
            {!props.project.protected ? (
              <>
                <p>
                  Off. Anyone who can open the project folder can read its transcripts. With a password, each transcript (and the key file) is sealed
                  with XChaCha20-Poly1305, under a key made from the password with Argon2id; the password itself is stored nowhere. Recordings are not
                  sealed, so full-disk encryption (BitLocker) is still recommended.
                </p>
                <NewPassword label="Protect the transcripts" busy={busy} needsConsent onSet={(pw) => setPassword(pw, 'The transcripts are sealed with the password.')} />
              </>
            ) : (
              <>
                <p>On. The transcripts are sealed; this window has them unlocked until you lock the project or close the app.</p>
                <div className="row">
                  <Button
                    isDisabled={busy}
                    onPress={async () => {
                      try {
                        props.onProject(await api.lock());
                        props.onClose();
                      } catch (e) {
                        props.onNotice(message(e));
                      }
                    }}
                  >
                    Lock now
                  </Button>
                  <Button isDisabled={busy} onPress={() => setChanging(!changing)} aria-expanded={changing}>
                    Change the password…
                  </Button>
                  <Button isDisabled={busy} onPress={() => setPassword(null, 'Password protection is off; the transcripts are plain files again.')}>
                    Turn protection off
                  </Button>
                </div>
                {changing && <NewPassword label="Change the password" busy={busy} needsConsent={false} onSet={(pw) => setPassword(pw, 'The password is changed.')} />}
              </>
            )}
          </section>
          <section aria-labelledby="statement-heading" className="privacy-section">
            <h3 id="statement-heading">Data handling statement</h3>
            <p className="hint">One page for an ethics protocol, written from this project's and this computer's set-up. It names no participant.</p>
            {statement && (
              <pre className="statement" tabIndex={0} aria-label="The data handling statement">
                {statement}
              </pre>
            )}
            <div className="row">
              <Button onPress={() => saveStatement('docx')}>Save as Word…</Button>
              <Button onPress={() => saveStatement('txt')}>Save as text…</Button>
            </div>
          </section>
          <div className="row end">
            <Button className="primary" isDisabled={busy} onPress={props.onClose}>
              Done
            </Button>
          </div>
        </Dialog>
      </Modal>
    </ModalOverlay>
  );
}
