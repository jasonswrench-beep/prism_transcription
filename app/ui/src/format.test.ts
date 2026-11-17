import { test } from 'node:test';
import assert from 'node:assert/strict';
import { duration, hms, progressText } from './format';

test('hms', () => {
  assert.equal(hms(0), '00:00:00');
  assert.equal(hms(3725.9), '01:02:05');
  assert.equal(hms(-3), '00:00:00');
});

test('duration', () => {
  assert.equal(duration(35), '35 s');
  assert.equal(duration(42 * 60), '42 min');
  assert.equal(duration(65 * 60), '1 h 05 min');
});

test('progress', () => {
  assert.equal(progressText({ stage: 'recognize', done: 0, total: 4, seconds: 0 }).text, 'Recognizing words: part 1 of 4');
  assert.equal(progressText({ stage: 'recognize', done: 4, total: 4, seconds: 0 }).text, 'Recognizing words: part 4 of 4');
  assert.equal(progressText({ stage: 'detect_voice' }).value, null);
  assert.equal(progressText({ stage: 'finished' }).value, 1);
});
