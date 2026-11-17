// The transcript editor (T4, T5). Turns are plain-text editable blocks; the word playing, words the recognizer was
// unsure of and search hits are marked with the CSS Custom Highlight API, so the text being typed is never re-drawn
// under the cursor. An edited turn is committed (its words retimed by the core's T5 rule) when the cursor leaves
// it, before saving, and before anything that changes the transcript as a whole.
import { memo, useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { Button } from 'react-aria-components';
import { api, type EditorInfo, type MediaFile, type ReviewStatus, type Transcript, type Turn } from '../api';
import { hms, LOW_CONFIDENCE } from '../format';
import { History } from './history';
import { actionFor, showKey, type Action } from './keys';
import { MediaPane, type PlayerState } from './MediaPane';
import { fillNotation, findIn, locate, timeline, turnText, wordAt, wordOffsets, type FindOptions } from './words';
import { FindBar, HotkeysDialog, NotationMenu, SnapshotsDialog } from './dialogs';

const REVIEW_WORDS: Record<ReviewStatus, string> = {
  not_started: 'Not started',
  first_pass: 'First pass',
  checked: 'Checked',
  de_identified: 'De-identified',
  final: 'Final',
};

// ---- Highlights -------------------------------------------------------------------------------------------

const highlights = (globalThis.CSS as unknown as { highlights?: Map<string, unknown> })?.highlights;

function rangeIn(el: HTMLElement, start: number, end: number): Range | null {
  const walker = document.createTreeWalker(el, NodeFilter.SHOW_TEXT);
  let at = 0;
  const r = document.createRange();
  let started = false;
  for (let n = walker.nextNode(); n; n = walker.nextNode()) {
    const len = n.textContent?.length ?? 0;
    if (!started && start <= at + len) {
      r.setStart(n, Math.max(0, start - at));
      started = true;
    }
    if (started && end <= at + len) {
      r.setEnd(n, Math.max(0, end - at));
      return r;
    }
    at += len;
  }
  return null;
}

function setHighlight(name: string, ranges: Range[]) {
  if (!highlights) return;
  if (ranges.length === 0) highlights.delete(name);
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  else highlights.set(name, new (globalThis as any).Highlight(...ranges));
}

function caretOffset(el: HTMLElement): number | null {
  const sel = window.getSelection();
  if (!sel || sel.rangeCount === 0 || !el.contains(sel.anchorNode)) return null;
  const r = document.createRange();
  r.selectNodeContents(el);
  r.setEnd(sel.anchorNode!, sel.anchorOffset);
  return r.toString().length;
}

// ---- One turn ---------------------------------------------------------------------------------------------

const TurnRow = memo(function TurnRow(props: {
  turn: Turn;
  index: number;
  name: string;
  colour: number;
  checked: boolean;
  clean: boolean[] | null;
  register: (i: number, el: HTMLDivElement | null) => void;
  onInput: (i: number, text: string) => void;
  onFocus: (i: number) => void;
  onBlur: (i: number) => void;
  onWordClick: (i: number, e: React.MouseEvent<HTMLDivElement>) => void;
}) {
  const { turn, index } = props;
  const text = props.clean ? turn.words.filter((_, i) => props.clean![i]).map((w) => w.text).join(' ') : turnText(turn);
  const ref = useRef<HTMLDivElement | null>(null);
  // Only write the text when it is not being edited, so the cursor never jumps.
  useEffect(() => {
    const el = ref.current;
    if (el && document.activeElement !== el && el.textContent !== text) el.textContent = text;
  }, [text]);
  return (
    <div className={`turn speaker-${props.colour}${props.checked ? ' checked' : ''}`} role="group" aria-label={`${props.name}, ${hms(turn.start)}`}>
      <div className="turn-head" aria-hidden="true">
        <span className="who">{props.name}</span>
        <span className="time">{hms(turn.start)}</span>
      </div>
      <div
        ref={(el) => {
          ref.current = el;
          if (el && !el.textContent) el.textContent = text;
          props.register(index, el);
        }}
        className="turn-text"
        contentEditable={props.clean ? false : 'plaintext-only'}
        suppressContentEditableWarning
        spellCheck
        role="textbox"
        aria-multiline="true"
        aria-label={`What ${props.name} said at ${hms(turn.start)}`}
        aria-readonly={props.clean ? true : undefined}
        tabIndex={0}
        onInput={(e) => props.onInput(index, (e.currentTarget.textContent ?? '').replace(/\s+/g, ' '))}
        onFocus={() => props.onFocus(index)}
        onBlur={() => props.onBlur(index)}
        onClick={(e) => props.onWordClick(index, e)}
        onKeyDown={(e) => {
          if (e.key === 'Enter') e.preventDefault(); // one paragraph per turn; splitting turns comes with speakers (phase 3)
        }}
      />
    </div>
  );
});

// ---- The editor -------------------------------------------------------------------------------------------

export function Editor(props: { file: MediaFile; transcript: Transcript; onClose: () => void; onSaved: () => void; onNotice: (s: string) => void }) {
  const [info, setInfo] = useState<EditorInfo | null>(null);
  const [peaks, setPeaks] = useState<number[] | null>(null);
  const [meta, setMeta] = useState<Transcript>(props.transcript);
  const history = useRef(new History<Turn[]>(props.transcript.turns));
  const [turns, setTurnsState] = useState<Turn[]>(props.transcript.turns);
  const [dirty, setDirty] = useState(false);
  const [savedAt, setSavedAt] = useState<Date | null>(null);
  const [player, setPlayer] = useState<PlayerState>({ time: 0, playing: false, rate: 1, boost: 1 });
  const [cleanView, setCleanView] = useState(false);
  const [masks, setMasks] = useState<boolean[][] | null>(null);
  const [findOpen, setFindOpen] = useState(false);
  const [snapOpen, setSnapOpen] = useState(false);
  const [keysOpen, setKeysOpen] = useState(false);
  const [notationOpen, setNotationOpen] = useState(false);
  const [hotkeys, setHotkeys] = useState<Record<string, string>>({});
  const [findState, setFindState] = useState<{ query: string; options: FindOptions; at: number }>({ query: '', options: { whole_word: false, match_case: false }, at: 0 });

  const mediaRef = useRef<HTMLVideoElement | null>(null);
  const els = useRef<(HTMLDivElement | null)[]>([]);
  const pending = useRef(new Map<number, string>());
  const active = useRef<number | null>(null);
  const savedRange = useRef<{ turn: number; start: number; end: number } | null>(null);
  const turnsRef = useRef(turns);
  turnsRef.current = turns;
  const metaRef = useRef(meta);
  metaRef.current = meta;
  const audio = useRef<{ ctx: AudioContext; gain: GainNode } | null>(null);
  const lastPause = useRef<number | null>(null);

  const speakerNumber = useCallback((id: string) => meta.speakers.findIndex((s) => s.id === id) + 1 || 1, [meta.speakers]);
  const names = useMemo(() => new Map(meta.speakers.map((s) => [s.id, s.name?.trim() || s.label])), [meta.speakers]);
  const tl = useMemo(() => timeline(turns), [turns]);
  const keyLabel = useCallback((a: string) => (hotkeys[a] ? showKey(hotkeys[a]) : ''), [hotkeys]);

  useEffect(() => {
    api.editorInfo(props.file.id).then((i) => {
      setInfo(i);
      setHotkeys(i.hotkeys);
    }, (e) => props.onNotice(String(e)));
    api.peaks(props.file.id).then(setPeaks, () => setPeaks(null));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [props.file.id]);

  const setTurns = useCallback((next: Turn[], record = true) => {
    if (record) history.current.push(next);
    setTurnsState(next);
    setDirty(true);
  }, []);

  // ---- committing and saving ----

  const commit = useCallback(async (i: number): Promise<Turn[]> => {
    const text = pending.current.get(i);
    const cur = turnsRef.current;
    if (text === undefined || !cur[i]) return cur;
    pending.current.delete(i);
    if (text.trim() === turnText(cur[i])) return cur;
    const lo = i > 0 ? cur[i - 1].end : 0;
    const hi = i + 1 < cur.length ? cur[i + 1].start : cur[i].end + 2;
    const t = await api.retimeTurn(cur[i], text, Math.min(lo, cur[i].start), Math.max(hi, cur[i].end));
    const latest = turnsRef.current;
    const next = latest.slice();
    if (t.words.length === 0) next.splice(i, 1);
    else next[i] = t;
    turnsRef.current = next;
    setTurns(next);
    return next;
  }, [setTurns]);

  const commitAll = useCallback(async () => {
    for (const i of [...pending.current.keys()].sort((a, b) => b - a)) await commit(i);
    return turnsRef.current;
  }, [commit]);

  const save = useCallback(async (quiet = false) => {
    const t = await commitAll();
    try {
      await api.saveTranscript({ ...metaRef.current, turns: t });
      setDirty(false);
      setSavedAt(new Date());
      props.onSaved();
    } catch (e) {
      if (!quiet) props.onNotice(`Could not save: ${String(e)}`);
    }
  }, [commitAll, props]);

  // Autosave every 30 seconds (T4).
  const dirtyRef = useRef(dirty);
  dirtyRef.current = dirty;
  useEffect(() => {
    const t = window.setInterval(() => {
      if (dirtyRef.current || pending.current.size) save(true);
    }, 30_000);
    return () => window.clearInterval(t);
  }, [save]);

  // Save when the window closes.
  useEffect(() => {
    let off: (() => void) | undefined;
    import('@tauri-apps/api/window').then(({ getCurrentWindow }) =>
      getCurrentWindow()
        .onCloseRequested(async () => {
          if (dirtyRef.current || pending.current.size) await save(true);
        })
        .then((u) => (off = u)),
    ).catch(() => {});
    return () => off?.();
  }, [save]);

  // ---- playback ----

  const media = () => mediaRef.current;

  const ensureBoost = useCallback((b: number) => {
    const m = media();
    if (!m) return;
    if (b !== 1 && !audio.current) {
      const ctx = new AudioContext();
      const src = ctx.createMediaElementSource(m);
      const gain = ctx.createGain();
      src.connect(gain).connect(ctx.destination);
      audio.current = { ctx, gain };
    }
    if (audio.current) audio.current.gain.gain.value = b;
    setPlayer((p) => ({ ...p, boost: b }));
  }, []);

  const play = useCallback((from?: number) => {
    const m = media();
    if (!m) return;
    if (from !== undefined) m.currentTime = Math.max(0, from);
    else if (lastPause.current !== null && info) m.currentTime = Math.max(0, m.currentTime - info.rewind_on_resume); // rewind on resume
    lastPause.current = null;
    audio.current?.ctx.resume();
    m.play().catch((e) => props.onNotice(`The recording could not be played: ${String(e)}`));
  }, [info, props]);

  const pause = useCallback(() => {
    const m = media();
    if (!m || m.paused) return;
    m.pause();
    lastPause.current = m.currentTime;
  }, []);

  const seek = useCallback((t: number) => {
    const m = media();
    if (!m) return;
    m.currentTime = Math.max(0, Math.min(t, (info?.duration ?? t) - 0.05));
    lastPause.current = null;
    setPlayer((p) => ({ ...p, time: m.currentTime }));
  }, [info]);

  const setRate = useCallback((r: number) => {
    const rate = Math.round(Math.min(2, Math.max(0.5, r)) * 10) / 10;
    const m = media();
    if (m) {
      m.preservesPitch = true;
      m.playbackRate = rate;
    }
    setPlayer((p) => ({ ...p, rate }));
  }, []);

  // Follow the playhead: the word playing is highlighted, and its turn kept in view unless someone is typing.
  const current = useRef<{ turn: number; word: number } | null>(null);
  useEffect(() => {
    let raf = 0;
    let last = 0;
    const tick = (now: number) => {
      const m = media();
      if (m) {
        const loc = locate(tl, m.currentTime);
        const prev = current.current;
        if (loc?.turn !== prev?.turn || loc?.word !== prev?.word) {
          current.current = loc;
          const el = loc ? els.current[loc.turn] : null;
          const t = loc ? turnsRef.current[loc.turn] : null;
          if (el && t && !pending.current.has(loc!.turn) && !cleanView) {
            const [a, b] = wordOffsets(t.words)[loc!.word];
            const r = rangeIn(el, a, b);
            setHighlight('pt-current', r ? [r] : []);
            if (!m.paused && active.current === null && loc!.turn !== prev?.turn) el.scrollIntoView({ block: 'nearest' });
          } else setHighlight('pt-current', []);
        }
        if (now - last > 100) {
          last = now;
          setPlayer((p) => (p.time === m.currentTime && p.playing === !m.paused ? p : { ...p, time: m.currentTime, playing: !m.paused }));
        }
      }
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, [tl, cleanView]);

  // Words the recognizer was unsure of (T2), in turns not being edited.
  useEffect(() => {
    const ranges: Range[] = [];
    if (!cleanView) {
      turns.forEach((t, i) => {
        const el = els.current[i];
        if (!el || pending.current.has(i)) return;
        const off = wordOffsets(t.words);
        t.words.forEach((w, j) => {
          if (w.confidence < LOW_CONFIDENCE) {
            const r = rangeIn(el, off[j][0], off[j][1]);
            if (r) ranges.push(r);
          }
        });
      });
    }
    setHighlight('pt-unsure', ranges);
  }, [turns, cleanView]);

  useEffect(() => () => ['pt-current', 'pt-unsure', 'pt-find', 'pt-find-current'].forEach((n) => setHighlight(n, [])), []);

  // ---- clean view ----

  useEffect(() => {
    if (!cleanView) return setMasks(null);
    commitAll().then((t) => api.cleanMasks(t).then(setMasks));
  }, [cleanView, turns, commitAll]);

  // ---- find ----

  const matches = useMemo(() => {
    if (!findOpen || !findState.query) return [] as { turn: number; start: number; end: number }[];
    const out: { turn: number; start: number; end: number }[] = [];
    turns.forEach((t, i) => findIn(turnText(t), findState.query, findState.options).forEach(([s, e]) => out.push({ turn: i, start: s, end: e })));
    return out;
  }, [findOpen, findState.query, findState.options, turns]);

  useEffect(() => {
    const all: Range[] = [];
    let cur: Range | null = null;
    matches.forEach((m, k) => {
      const el = els.current[m.turn];
      const r = el ? rangeIn(el, m.start, m.end) : null;
      if (r) {
        all.push(r);
        if (k === findState.at % Math.max(1, matches.length)) cur = r;
      }
    });
    setHighlight('pt-find', all);
    setHighlight('pt-find-current', cur ? [cur] : []);
    const m = matches[findState.at % Math.max(1, matches.length)];
    if (m) els.current[m.turn]?.scrollIntoView({ block: 'center' });
  }, [matches, findState.at]);

  const replaceAll = async (replace: string, wholeProject: boolean) => {
    if (wholeProject) {
      await save();
      const r = await api.replaceInProject(findState.query, replace, findState.options);
      const fresh = await api.transcript(props.file.id);
      if (fresh) {
        setTurns(fresh.turns);
        setDirty(false);
      }
      const n = r.reduce((s, [, c]) => s + c, 0);
      props.onNotice(n ? `Replaced ${n} in ${r.length} transcript${r.length === 1 ? '' : 's'}. Other transcripts were saved as they changed and cannot be undone from here.` : 'Nothing found to replace.');
      return;
    }
    const cur = await commitAll();
    const next = cur.slice();
    let n = 0;
    for (let i = 0; i < cur.length; i++) {
      const text = turnText(cur[i]);
      const hits = findIn(text, findState.query, findState.options);
      if (!hits.length) continue;
      n += hits.length;
      let out = '';
      let at = 0;
      for (const [s, e] of hits) {
        out += text.slice(at, s) + replace;
        at = e;
      }
      out += text.slice(at);
      const lo = i > 0 ? cur[i - 1].end : 0;
      const hi = i + 1 < cur.length ? cur[i + 1].start : cur[i].end + 2;
      next[i] = await api.retimeTurn(cur[i], out, Math.min(lo, cur[i].start), Math.max(hi, cur[i].end));
    }
    if (n) setTurns(next.filter((t) => t.words.length > 0));
    props.onNotice(n ? `Replaced ${n} in this transcript. Undo (Ctrl+Z) brings them back.` : 'Nothing found to replace.');
  };

  // ---- inserting at the cursor ----

  const rememberCursor = () => {
    const i = active.current;
    const el = i !== null ? els.current[i] : null;
    const sel = window.getSelection();
    if (i === null || !el || !sel || sel.rangeCount === 0) return (savedRange.current = null);
    const start = caretOffset(el) ?? 0;
    savedRange.current = { turn: i, start, end: start + sel.toString().length };
  };

  const insertText = (text: string, select: [number, number] | null) => {
    const s = savedRange.current;
    const el = s ? els.current[s.turn] : null;
    if (!s || !el) {
      props.onNotice('Put the cursor in a turn first, then insert.');
      return;
    }
    el.focus();
    const r = rangeIn(el, s.start, s.end);
    if (r) {
      const sel = window.getSelection()!;
      sel.removeAllRanges();
      sel.addRange(r);
    }
    const before = s.start > 0 && !/\s$/.test((el.textContent ?? '').slice(0, s.start)) ? ' ' : '';
    document.execCommand('insertText', false, before + text + ' ');
    if (select) {
      const at = s.start + before.length;
      const rr = rangeIn(el, at + select[0], at + select[1]);
      if (rr) {
        const sel = window.getSelection()!;
        sel.removeAllRanges();
        sel.addRange(rr);
      }
    }
  };

  // ---- actions (hotkeys and buttons) ----

  const caretTime = (): number => {
    const i = active.current;
    const el = i !== null ? els.current[i] : null;
    const t = i !== null ? turnsRef.current[i] : null;
    if (!el || !t) return media()?.currentTime ?? 0;
    const off = caretOffset(el);
    if (off === null) return media()?.currentTime ?? 0;
    if (pending.current.has(i!)) {
      // The words are not lined up with typed text yet: place the cursor by its share of the turn's text.
      const len = Math.max(1, (el.textContent ?? '').length);
      return t.start + (t.end - t.start) * Math.min(1, off / len);
    }
    return t.words[wordAt(t.words, off)]?.end ?? t.end;
  };

  const markChecked = () => {
    const until = active.current !== null ? caretTime() : (media()?.currentTime ?? 0);
    setMeta((m) => ({ ...m, checked_until: until, review: m.review === 'not_started' ? 'first_pass' : m.review }));
    setDirty(true);
    props.onNotice(`Marked as checked up to ${hms(until)}.`);
  };

  const focusTurn = async (i: number) => {
    if (active.current !== null) await commit(active.current);
    const el = els.current[Math.max(0, Math.min(i, turnsRef.current.length - 1))];
    if (!el) return;
    el.focus();
    const sel = window.getSelection()!;
    sel.removeAllRanges();
    const r = document.createRange();
    r.selectNodeContents(el);
    r.collapse(true);
    sel.addRange(r);
    el.scrollIntoView({ block: 'nearest' });
  };

  const run = (a: Action) => {
    const m = media();
    switch (a) {
      case 'play_pause':
        if (m && m.paused) play();
        else pause();
        break;
      case 'back':
        seek((m?.currentTime ?? 0) - 2);
        break;
      case 'forward':
        seek((m?.currentTime ?? 0) + 2);
        break;
      case 'slower':
        setRate(player.rate - 0.1);
        break;
      case 'faster':
        setRate(player.rate + 0.1);
        break;
      case 'timestamp':
        rememberCursor();
        insertText(`[${hms(m?.currentTime ?? 0)}]`, null);
        break;
      case 'notation':
        rememberCursor();
        setNotationOpen(true);
        break;
      case 'mark_checked':
        markChecked();
        break;
      case 'find':
        setFindOpen(true);
        break;
      case 'snapshot':
        setSnapOpen(true);
        break;
      case 'next_turn':
        focusTurn((active.current ?? -1) + 1);
        break;
      case 'previous_turn':
        focusTurn((active.current ?? 1) - 1);
        break;
      case 'play_turn': {
        const t = active.current !== null ? turnsRef.current[active.current] : null;
        if (t) play(t.start);
        break;
      }
    }
  };
  const runRef = useRef(run);
  runRef.current = run;

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (keysOpen) return; // the remap screen is listening for a key
      const ctrl = e.ctrlKey || e.metaKey;
      // Undo and redo: typing inside a turn uses the browser's own undo until the turn is committed.
      if (ctrl && !e.altKey && (e.key.toLowerCase() === 'z' || e.key.toLowerCase() === 'y')) {
        const redo = e.key.toLowerCase() === 'y' || e.shiftKey;
        if (active.current !== null && pending.current.has(active.current)) return;
        e.preventDefault();
        const next = redo ? history.current.redo() : history.current.undo();
        setTurns(next, false);
        return;
      }
      if (ctrl && e.key.toLowerCase() === 's' && !e.shiftKey) {
        e.preventDefault();
        save();
        return;
      }
      const a = actionFor(e, hotkeys);
      if (a) {
        e.preventDefault();
        e.stopPropagation();
        runRef.current(a);
      }
    };
    window.addEventListener('keydown', onKey, true);
    return () => window.removeEventListener('keydown', onKey, true);
  }, [hotkeys, keysOpen, save, setTurns]);

  // ---- turn callbacks (stable, so unchanged turns are not re-rendered) ----

  const register = useCallback((i: number, el: HTMLDivElement | null) => {
    els.current[i] = el;
  }, []);
  const onInput = useCallback((i: number, text: string) => {
    pending.current.set(i, text);
    setDirty(true);
  }, []);
  const onFocus = useCallback((i: number) => {
    active.current = i;
  }, []);
  const onBlur = useCallback((i: number) => {
    if (active.current === i) active.current = null;
    commit(i);
  }, [commit]);
  const onWordClick = useCallback((i: number, e: React.MouseEvent<HTMLDivElement>) => {
    const t = turnsRef.current[i];
    const el = els.current[i];
    if (!t || !el || pending.current.has(i) || cleanView) return;
    const off = caretOffset(el);
    if (off === null) return;
    const w = wordAt(t.words, off);
    if (e.ctrlKey || e.metaKey) {
      // Ctrl+click selects the word without playing (T4).
      const [a, b] = wordOffsets(t.words)[w];
      const r = rangeIn(el, a, b);
      if (r) {
        const sel = window.getSelection()!;
        sel.removeAllRanges();
        sel.addRange(r);
      }
      return;
    }
    play(t.words[w].start);
  }, [cleanView, play]);

  const checkedUntil = meta.checked_until ?? null;
  const checkedTurns = turns.reduce((n, t) => n + (checkedUntil !== null && t.end <= checkedUntil ? 1 : 0), 0);

  return (
    <div className="editor">
      <div className="editor-bar">
        <Button onPress={async () => { await save(true); props.onClose(); }} aria-label="Back to all recordings">
          ← Recordings
        </Button>
        <h2 className="editor-title">{props.file.label}</h2>
        <label className="review">
          Review
          <select
            value={meta.review}
            onChange={(e) => {
              setMeta((m) => ({ ...m, review: e.target.value as ReviewStatus }));
              setDirty(true);
            }}
          >
            {Object.entries(REVIEW_WORDS).map(([k, v]) => (
              <option key={k} value={k}>
                {v}
              </option>
            ))}
          </select>
        </label>
        <Button onPress={() => setCleanView((c) => !c)} aria-pressed={cleanView}>
          {cleanView ? 'Clean view (read-only)' : 'Verbatim'}
        </Button>
        <Button onPress={() => { rememberCursor(); setNotationOpen(true); }} aria-label={`Notation (${keyLabel('notation')})`}>
          Notation
        </Button>
        <Button onPress={() => setFindOpen(true)} aria-label={`Find and replace (${keyLabel('find')})`}>
          Find
        </Button>
        <Button onPress={() => setSnapOpen(true)}>Snapshots</Button>
        <Button onPress={() => setKeysOpen(true)}>Keys</Button>
        <span className="save-state" role="status">
          {dirty ? 'Unsaved changes' : savedAt ? `Saved ${savedAt.toLocaleTimeString()}` : 'Saved'}
        </span>
        <Button onPress={() => save()} isDisabled={!dirty}>
          Save
        </Button>
      </div>
      {findOpen && (
        <FindBar
          state={findState}
          count={matches.length}
          onChange={(s) => setFindState(s)}
          onNext={(d) => setFindState((s) => ({ ...s, at: (s.at + d + Math.max(1, matches.length)) % Math.max(1, matches.length) }))}
          onReplaceAll={replaceAll}
          onClose={() => setFindOpen(false)}
        />
      )}
      <div className="editor-panes">
        <section className="turn-list" aria-label="Transcript, one turn per paragraph">
          <p className="hint">
            {checkedUntil !== null ? `Checked up to ${hms(checkedUntil)} (${checkedTurns} of ${turns.length} turns). ` : ''}
            Click a word to play from it; Ctrl+click selects it. {cleanView ? 'The clean view hides fillers without deleting them; switch to Verbatim to edit.' : ''}
          </p>
          {turns.map((t, i) => (
            <TurnRow
              key={`${i}-${t.start}`}
              turn={t}
              index={i}
              name={names.get(t.speaker) ?? t.speaker}
              colour={((speakerNumber(t.speaker) - 1) % 8) + 1}
              checked={checkedUntil !== null && t.end <= checkedUntil}
              clean={cleanView && masks ? masks[i] ?? null : null}
              register={register}
              onInput={onInput}
              onFocus={onFocus}
              onBlur={onBlur}
              onWordClick={onWordClick}
            />
          ))}
        </section>
        {info && (
          <MediaPane
            info={info}
            peaks={peaks}
            turns={turns}
            speakerNumber={speakerNumber}
            checkedUntil={checkedUntil}
            player={player}
            mediaRef={mediaRef}
            onSeek={seek}
            onTogglePlay={() => run('play_pause')}
            onSkip={(s) => seek((media()?.currentTime ?? 0) + s)}
            onRate={setRate}
            onBoost={ensureBoost}
            keyLabel={keyLabel}
          />
        )}
      </div>
      <NotationMenu
        isOpen={notationOpen}
        entries={info?.notation ?? []}
        onClose={() => setNotationOpen(false)}
        onPick={(entry) => {
          setNotationOpen(false);
          const { text, select } = fillNotation(entry, media()?.currentTime ?? 0, hms);
          setTimeout(() => insertText(text, select), 0);
        }}
        onSaveList={async (list) => {
          const saved = await api.setNotation(list);
          setInfo((i) => (i ? { ...i, notation: saved } : i));
        }}
      />
      <SnapshotsDialog
        isOpen={snapOpen}
        snapshots={meta.snapshots}
        onClose={() => setSnapOpen(false)}
        onSave={async (name) => {
          const t = await commitAll();
          setMeta((m) => ({ ...m, snapshots: [...m.snapshots, { name, created: new Date().toISOString(), turns: t }] }));
          setDirty(true);
          props.onNotice(`Snapshot "${name}" saved.`);
        }}
        onRestore={async (k) => {
          await commitAll();
          setTurns(meta.snapshots[k].turns);
          setSnapOpen(false);
          props.onNotice(`Restored "${meta.snapshots[k].name}". Undo (Ctrl+Z) goes back to what you had.`);
        }}
      />
      <HotkeysDialog
        isOpen={keysOpen}
        keys={hotkeys}
        rewind={info?.rewind_on_resume ?? 1.5}
        onClose={() => setKeysOpen(false)}
        onSave={async (keys, rewind) => {
          setHotkeys(await api.setHotkeys(keys, rewind));
          setInfo((i) => (i ? { ...i, rewind_on_resume: rewind } : i));
          setKeysOpen(false);
        }}
      />
    </div>
  );
}
