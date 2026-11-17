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
