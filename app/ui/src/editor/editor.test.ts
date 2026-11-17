import { test } from 'node:test';
import assert from 'node:assert/strict';
import { actionFor, clashes, keyName, showKey } from './keys';
import { History } from './history';
import { fillNotation, findIn, locate, timeline, wordAt, wordOffsets } from './words';
import type { Turn, Word } from '../api';

const k = (key: string, mods: Partial<{ ctrlKey: boolean; altKey: boolean; shiftKey: boolean; metaKey: boolean }> = {}) => ({
  key,
  ctrlKey: false,
  altKey: false,
  shiftKey: false,
  metaKey: false,
  ...mods,
});

test('key names', () => {
  assert.equal(keyName(k('F4')), 'F4');
  assert.equal(keyName(k('T', { ctrlKey: true, shiftKey: true })), 'Control+Shift+t');
  assert.equal(keyName(k('t', { ctrlKey: true })), 'Control+t');
  assert.equal(keyName(k('?', { shiftKey: true })), '?', 'shift is part of the character');
  assert.equal(keyName(k('Control', { ctrlKey: true })), null);
  assert.equal(keyName(k(' ')), 'Space');
  assert.equal(showKey('Control+Shift+s'), 'Ctrl+Shift+S');
  assert.equal(showKey('Control+ArrowDown'), 'Ctrl+Down');
});

test('actions and clashes', () => {
  const keys = { play_pause: 'F4', timestamp: 'Control+t' };
  assert.equal(actionFor(k('F4'), keys), 'play_pause');
  assert.equal(actionFor(k('t', { ctrlKey: true }), keys), 'timestamp');
  assert.equal(actionFor(k('t'), keys), null);
  assert.deepEqual(clashes({ a: 'F4', b: 'F5', c: 'F4' }), ['F4']);
});

test('history', () => {
  const h = new History([1]);
  h.push([1, 2]);
  h.push([1, 2, 3]);
  assert.deepEqual(h.undo(), [1, 2]);
  assert.deepEqual(h.undo(), [1]);
  assert.deepEqual(h.undo(), [1], 'nothing more to undo');
  assert.deepEqual(h.redo(), [1, 2]);
  h.push([9]);
  assert.equal(h.canRedo, false, 'a new change clears redo');
});

const w = (text: string, start: number, end: number): Word => ({ text, start, end, confidence: 1, speaker: 'S1' });

test('offsets and the word at a position', () => {
  const words = [w('Hello,', 0, 0.4), w('Maria.', 0.5, 0.9)];
  assert.deepEqual(wordOffsets(words), [[0, 6], [7, 13]]);
  assert.equal(wordAt(words, 3), 0);
  assert.equal(wordAt(words, 7), 1);
  assert.equal(wordAt(words, 99), 1);
});

test('the word playing now', () => {
  const turns: Turn[] = [
    { speaker: 'S1', start: 0, end: 1, words: [w('a', 0, 0.5), w('b', 0.5, 1)] },
    { speaker: 'S2', start: 5, end: 6, words: [w('c', 5, 6)] },
  ];
  const tl = timeline(turns);
  assert.deepEqual(locate(tl, 0.7), { turn: 0, word: 1 });
  assert.deepEqual(locate(tl, 5.2), { turn: 1, word: 0 });
  assert.equal(locate(tl, 3.5), null, 'a long pause');
  assert.equal(locate(tl, -1), null);
});

test('find, as the core replaces', () => {
  assert.equal(findIn("Maria met maria's friend, Mariana.", 'maria', { whole_word: false, match_case: false }).length, 3);
  assert.deepEqual(findIn("Maria met maria's friend, Mariana.", 'maria', { whole_word: true, match_case: false }), [[0, 5], [10, 15]]);
  assert.deepEqual(findIn('Maria met maria.', 'Maria', { whole_word: true, match_case: true }), [[0, 5]]);
});

test('notation templates', () => {
  const hms = (t: number) => `00:00:${String(Math.floor(t)).padStart(2, '0')}`;
  assert.deepEqual(fillNotation('[inaudible hh:mm:ss]', 12.4, hms), { text: '[inaudible 00:00:12]', select: null });
  assert.deepEqual(fillNotation('[unclear: …]', 0, hms), { text: '[unclear: …]', select: [10, 11] });
});
