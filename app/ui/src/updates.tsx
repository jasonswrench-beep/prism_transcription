// Check for updates (docs/UPDATES.md): a button, never a timer. One request when pressed; installing is a second step with a go-ahead, and
// the download is checked against the update key built into the program before it runs. Refused while Offline is on.
import { useEffect, useState } from 'react';
import { Button, ProgressBar } from 'react-aria-components';
import { api, on, type UpdateFound, type UpdateState } from './api';
import { errorText } from './format';

function message(e: unknown): string {
  return errorText(e);
}

/** Ask an open editor to save now, and wait for it (the installer closes the program). */
export async function saveOpenWork(): Promise<void> {
  const wait: Promise<unknown>[] = [];
  window.dispatchEvent(new CustomEvent('prism-save-now', { detail: { wait } }));
  await Promise.all(wait);
}

export function UpdatesCard() {
  const [state, setState] = useState<UpdateState | null>(null);
  const [found, setFound] = useState<UpdateFound | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<'checking' | 'confirm' | 'installing' | null>(null);
  const [progress, setProgress] = useState<{ received: number; total: number | null } | null>(null);
  useEffect(() => {
    api.updateState().then(setState).catch(() => setState(null));
    const off = on<{ received: number; total: number | null }>('update-progress', setProgress);
    return () => {
      off.then((f) => f());
    };
  }, []);
  if (!state) return null;
  const check = async () => {
    setBusy('checking');
    setError(null);
    setFound(null);
    try {
      setFound(await api.updateCheck());
    } catch (e) {
      setError(message(e));
    } finally {
      setBusy(null);
    }
  };
  const install = async () => {
    setBusy('installing');
    setError(null);
    try {
      await saveOpenWork();
      const r = await api.updateInstall();
      if (!r.installed) setError('There is no newer version now.');
    } catch (e) {
      setError(message(e));
    } finally {
      setBusy(null);
    }
  };
  const blockedWords =
    state.blocked === 'not_set_up'
      ? 'This version is not set up to check for updates. Prism Hub installs newer versions, or download one from the project page.'
      : state.blocked === 'offline'
        ? 'Offline is on, so Prism Transcribe does not check. Turn Offline off to check.'
        : null;
  return (
    <section className="updates" aria-labelledby="updates-heading">
      <h3 id="updates-heading">Updates</h3>
      <p>
        This is Prism Transcribe {state.version}. Checking asks {state.host || 'the project page'} once for the newest version; nothing is checked
        on its own.
      </p>
      {blockedWords ? (
        <p className="hint">{blockedWords}</p>
      ) : (
        <div className="row">
          <Button onPress={check} isDisabled={busy !== null}>
            {busy === 'checking' ? 'Checking…' : 'Check for updates'}
          </Button>
        </div>
      )}
      {found && !found.available && <p role="status">Prism Transcribe is up to date.</p>}
      {found?.available && (
        <div className="update-found" role="status">
          <p>
            <strong>Version {found.version} is available.</strong>
            {found.date ? ` Published ${found.date.split(' ')[0]}.` : ''}
          </p>
          {found.notes && <pre className="statement">{found.notes}</pre>}
          {busy === 'confirm' ? (
            <div className="row">
              <span>Install now? Prism Transcribe saves your work, closes, installs the new version for this Windows account, and opens again.</span>
              <Button className="primary" onPress={install}>
                Install
              </Button>
              <Button onPress={() => setBusy(null)}>Not now</Button>
            </div>
          ) : busy === 'installing' ? (
            <ProgressBar aria-label="Downloading the update" value={progress?.total ? (progress.received / progress.total) * 100 : undefined} isIndeterminate={!progress?.total}>
              <span className="job-text">Downloading and checking its signature…</span>
              <div className="bar">
                <div className="fill" style={{ width: progress?.total ? `${(progress.received / progress.total) * 100}%` : '30%' }} />
              </div>
            </ProgressBar>
          ) : (
            <Button className="primary" onPress={() => setBusy('confirm')}>
              Download and install…
            </Button>
          )}
        </div>
      )}
      {error && (
        <p className="error" role="alert">
          {error}
        </p>
      )}
    </section>
  );
}
