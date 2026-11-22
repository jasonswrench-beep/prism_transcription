import { test } from 'node:test';
import assert from 'node:assert/strict';
import { duration, errorText, hms, languageCode, progressText, sentence } from './format';

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

test('messages read as sentences', () => {
  assert.equal(sentence('the zip could not be read'), 'The zip could not be read.');
  assert.equal(sentence('Done.'), 'Done.');
  assert.equal(errorText(new Error('no speech was heard in Tone')), 'No speech was heard in Tone.');
  assert.equal(errorText(''), '');
});

test('the language box takes a code or a name', () => {
  assert.equal(languageCode(''), '');
  assert.equal(languageCode(' EN '), 'en');
  assert.equal(languageCode('english'), 'en');
  assert.equal(languageCode('Japanese'), 'ja');
  assert.equal(languageCode('yue'), 'yue');
  assert.equal(languageCode('e'), null, 'one letter is not a code');
  assert.equal(languageCode('englsh'), null);
});

test('command arguments are camelCase, as Tauri reads them', async () => {
  // Tauri turns a Rust argument rewind_on_resume into rewindOnResume; a snake_case key is refused (the Keys bug).
  const { readFileSync } = await import('node:fs');
  const src = readFileSync(new URL('./api.ts', import.meta.url), 'utf8');
  const calls = [...src.matchAll(/invoke<[^(]*>\('([a-z_]+)'(?:, \{([^}]*)\})?\)/g)];
  assert.ok(calls.length > 40, `found ${calls.length} calls`);
  assert.ok(calls.some(([, c, a]) => c === 'hotkeys_set' && a?.includes('rewindOnResume')), 'the Keys call is among them');
  for (const [, command, args] of calls) {
    for (const key of (args ?? '').split(',').map((k) => k.split(':')[0].trim()).filter(Boolean)) {
      assert.ok(!key.includes('_'), `${command}: argument ${key} must be camelCase`);
    }
  }
});
