// Hotkeys (T4): every action has a key, all remappable. Keys are written "Control+Shift+s", modifiers first, the key
// as the browser names it (KeyboardEvent.key), letters lower case. A USB foot pedal that presents as a keyboard sends
// ordinary keys, so the remap screen covers it.

export type Action =
  | 'play_pause'
  | 'back'
  | 'forward'
  | 'slower'
  | 'faster'
  | 'timestamp'
  | 'notation'
  | 'mark_checked'
  | 'find'
  | 'snapshot'
  | 'next_turn'
  | 'previous_turn'
  | 'play_turn';

export const ACTION_LABELS: Record<Action, string> = {
  play_pause: 'Play or pause',
  back: 'Back 2 seconds',
  forward: 'Forward 2 seconds',
  slower: 'Slower',
  faster: 'Faster',
  timestamp: 'Insert a timestamp',
  notation: 'Notation menu',
  mark_checked: 'Mark checked up to here',
  find: 'Find and replace',
  snapshot: 'Save a snapshot',
  next_turn: 'Next turn',
  previous_turn: 'Previous turn',
  play_turn: 'Play this turn from its start',
};

export interface KeyLike {
  key: string;
  ctrlKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
  metaKey: boolean;
}

const MODIFIER_KEYS = new Set(['Control', 'Alt', 'Shift', 'Meta', 'AltGraph']);

/** The key's name, or null for a modifier pressed on its own. */
export function keyName(e: KeyLike): string | null {
  if (MODIFIER_KEYS.has(e.key)) return null;
  const mods: string[] = [];
  if (e.ctrlKey) mods.push('Control');
  if (e.altKey) mods.push('Alt');
  // Shift is part of a printable character's own name ("?"), so it is only spelled out for letters and named keys.
  const k = e.key.length === 1 ? e.key.toLowerCase() : e.key;
  if (e.shiftKey && (e.key.length > 1 || /[a-z]/i.test(e.key))) mods.push('Shift');
  if (e.metaKey) mods.push('Meta');
  return [...mods, k === ' ' ? 'Space' : k].join('+');
}

/** Which action a key press means, if any. */
export function actionFor(e: KeyLike, keys: Record<string, string>): Action | null {
  const name = keyName(e);
  if (!name) return null;
  for (const [action, key] of Object.entries(keys)) {
    if (key === name) return action as Action;
  }
  return null;
}

/** A key name as people read it: "Ctrl+Shift+S", "F4". */
export function showKey(k: string): string {
  return k
    .split('+')
    .map((p) => (p === 'Control' ? 'Ctrl' : p.length === 1 ? p.toUpperCase() : p.replace('Arrow', '')))
    .join('+');
}

/** Two actions on one key would make one of them unreachable. */
export function clashes(keys: Record<string, string>): string[] {
  const seen = new Map<string, string>();
  const out: string[] = [];
  for (const [a, k] of Object.entries(keys)) {
    if (seen.has(k)) out.push(k);
    else seen.set(k, a);
  }
  return out;
}
