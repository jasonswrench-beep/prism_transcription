// Small formatting helpers shared by the screens.
import type { Progress } from './api';

/** hh:mm:ss, the transcript's time format (T5). */
export function hms(seconds: number): string {
  const s = Math.max(0, Math.floor(seconds));
  const pad = (n: number) => String(n).padStart(2, '0');
  return `${pad(Math.floor(s / 3600))}:${pad(Math.floor(s / 60) % 60)}:${pad(s % 60)}`;
}

/** "42 min", "1 h 05 min", "35 s". */
export function duration(seconds: number): string {
  if (seconds < 60) return `${Math.round(seconds)} s`;
  const m = Math.round(seconds / 60);
  if (m < 60) return `${m} min`;
  return `${Math.floor(m / 60)} h ${String(m % 60).padStart(2, '0')} min`;
}

/** A job's progress as a fraction (0..1) and a sentence, for the progress bar and screen readers. */
export function progressText(p: Progress | null): { value: number | null; text: string } {
  if (!p) return { value: null, text: 'Starting…' };
  switch (p.stage) {
    case 'detect_voice':
      return { value: null, text: 'Finding the speech…' };
    case 'recognize':
      return { value: p.total ? (p.done / p.total) * 0.85 : 0, text: `Recognizing words: part ${Math.min(p.done + 1, p.total)} of ${p.total}` };
    case 'label_speakers':
      return { value: 0.9, text: 'Telling the speakers apart…' };
    case 'finished':
      return { value: 1, text: 'Finished' };
  }
}

/** The tint threshold for words the recognizer was unsure of (T2, open decision 12). */
export const LOW_CONFIDENCE = 0.6;

/** A message as a sentence: a capital letter first and a full stop at the end. The program's own error words
 * start in lower case (they are also used inside other sentences). */
export function sentence(text: string): string {
  const t = text.trim();
  if (!t) return t;
  const c = t.charAt(0).toUpperCase() + t.slice(1);
  return /[.!?…)]$/.test(c) ? c : `${c}.`;
}

/** What went wrong, as a sentence, from whatever a command threw. */
export function errorText(e: unknown): string {
  return sentence(errorWords(e));
}

/** What a command threw, as it is, for use inside a longer sentence. */
export function errorWords(e: unknown): string {
  return typeof e === 'string' ? e : e instanceof Error ? e.message : String(e);
}

/** Language names people type, and the code they mean. */
const LANGUAGE_NAMES: Record<string, string> = {
  english: 'en', french: 'fr', german: 'de', spanish: 'es', italian: 'it', portuguese: 'pt', dutch: 'nl', danish: 'da', swedish: 'sv',
  norwegian: 'no', finnish: 'fi', polish: 'pl', czech: 'cs', slovak: 'sk', slovenian: 'sl', croatian: 'hr', hungarian: 'hu', romanian: 'ro',
  bulgarian: 'bg', greek: 'el', estonian: 'et', latvian: 'lv', lithuanian: 'lt', maltese: 'mt', russian: 'ru', ukrainian: 'uk', turkish: 'tr',
  arabic: 'ar', hebrew: 'he', persian: 'fa', farsi: 'fa', hindi: 'hi', urdu: 'ur', bengali: 'bn', punjabi: 'pa', tamil: 'ta', chinese: 'zh',
  mandarin: 'zh', cantonese: 'yue', japanese: 'ja', korean: 'ko', vietnamese: 'vi', thai: 'th', indonesian: 'id', malay: 'ms', tagalog: 'tl',
  filipino: 'tl', swahili: 'sw', welsh: 'cy', irish: 'ga', icelandic: 'is', catalan: 'ca', basque: 'eu', galician: 'gl',
};

/** The language box: '' for automatic, a code, or null when it is neither a code nor a known name. */
export function languageCode(typed: string): string | null {
  const t = typed.trim().toLowerCase();
  if (!t) return '';
  if (LANGUAGE_NAMES[t]) return LANGUAGE_NAMES[t];
  return /^[a-z]{2,3}$/.test(t) ? t : null;
}
