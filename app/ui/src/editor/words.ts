// Where each word sits in a turn's text, which word plays at a time, and finding text. A turn's displayed text is
// its words joined by single spaces, so a word's place in the text follows from the words before it.
import type { Turn, Word } from '../api';

/** [start, end) offsets (UTF-16, as the browser counts) of each word in `words.map(w => w.text).join(' ')`. */
export function wordOffsets(words: Word[]): [number, number][] {
  const out: [number, number][] = [];
  let at = 0;
  for (const w of words) {
    out.push([at, at + w.text.length]);
    at += w.text.length + 1;
  }
  return out;
}

export function turnText(t: Turn): string {
  return t.words.map((w) => w.text).join(' ');
}

/** The word at a text offset (the one the offset falls in, or the next one after a space). */
export function wordAt(words: Word[], offset: number): number {
  const off = wordOffsets(words);
  for (let i = 0; i < off.length; i++) if (offset <= off[i][1]) return i;
  return Math.max(0, off.length - 1);
}

export interface Located {
  turn: number;
  word: number;
}

/** Every word's start time in order, for finding the word playing now by binary search. */
export function timeline(turns: Turn[]): { start: number; end: number; turn: number; word: number }[] {
  const out: { start: number; end: number; turn: number; word: number }[] = [];
  turns.forEach((t, ti) => t.words.forEach((w, wi) => out.push({ start: w.start, end: w.end, turn: ti, word: wi })));
  // Turns transcribed track by track can overlap in time (crosstalk), so order by time for the search.
  for (let i = 1; i < out.length; i++) {
    if (out[i].start < out[i - 1].start) {
      out.sort((a, b) => a.start - b.start);
      break;
    }
  }
  return out;
}

/** The word playing at `time`: the last one that started at or before it, if it has not ended long before. */
export function locate(tl: ReturnType<typeof timeline>, time: number): Located | null {
  let lo = 0;
  let hi = tl.length - 1;
  let found = -1;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    if (tl[mid].start <= time) {
      found = mid;
      lo = mid + 1;
    } else hi = mid - 1;
  }
  if (found < 0) return null;
  const w = tl[found];
  if (time > w.end + 1.0) return null; // in a pause: nothing is being said
  return { turn: w.turn, word: w.word };
}

export interface FindOptions {
  whole_word: boolean;
  match_case: boolean;
}

const WORD_CHAR = /[\p{L}\p{N}_]/u;

/** Where `find` occurs in `text`, by the same rule as the core's replace (an apostrophe is a word boundary, so
 * "Maria" is found in "Maria's"). */
export function findIn(text: string, find: string, o: FindOptions): [number, number][] {
  if (!find) return [];
  const hay = o.match_case ? text : text.toLowerCase();
  const needle = o.match_case ? find : find.toLowerCase();
  const out: [number, number][] = [];
  let i = hay.indexOf(needle);
  while (i >= 0) {
    const end = i + needle.length;
    const ok =
      !o.whole_word ||
      ((!WORD_CHAR.test(text[i] ?? '') || !WORD_CHAR.test(text[i - 1] ?? '')) && (!WORD_CHAR.test(text[end - 1] ?? '') || !WORD_CHAR.test(text[end] ?? '')));
    if (ok) out.push([i, end]);
    i = hay.indexOf(needle, i + Math.max(1, needle.length));
  }
  return out;
}

/** hh:mm:ss for a notation entry ("[inaudible hh:mm:ss]") and where the cursor goes ("…" is selected). */
export function fillNotation(template: string, time: number, hms: (t: number) => string): { text: string; select: [number, number] | null } {
  const text = template.replace('hh:mm:ss', hms(time));
  const at = text.indexOf('…');
  return { text, select: at >= 0 ? [at, at + 1] : null };
}
