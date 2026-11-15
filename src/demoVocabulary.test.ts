import { expect, test } from 'vitest';
import { demoDetail, demoLibraries, demoWords, searchDemo } from './demoVocabulary';
import type { WordQuery } from './vocabulary';

const query: WordQuery = { text: '', library_id: null, unclassified: false, jlpt: null, pos: null, state: null, due: null, sort: 'updated_at', descending: true };

const spellings = ['学校', '先生', '日本語', '食べる', '勉強', 'おはよう', 'ありがとう', 'お茶'];

test('browser samples retain their identifiers, memberships, ordering and list metadata', () => {
  expect(demoWords.map(word => word.kanji)).toEqual(spellings);
  expect(searchDemo(query).map(word => word.id)).toEqual(['demo-7', 'demo-6', 'demo-5', 'demo-4', 'demo-3', 'demo-2', 'demo-1', 'demo-0']);
  expect(searchDemo({ ...query, descending: false })).toEqual(demoWords);
  demoLibraries.forEach(library => {
    expect(searchDemo({ ...query, library_id: library.id })).toHaveLength(library.word_count);
  });
  demoWords.forEach((word, index) => {
    expect(word).toMatchObject({ id: `demo-${index}`, revision: 1, updated_at: index, jlpt: 'N5', card_state: null, due_at: null, suspended: false, buried_until: null });
    expect(demoDetail(word.id).content.library_ids).toEqual([index < 5 ? 'demo-basic' : 'demo-life']);
  });
  expect(demoDetail('demo-0').content.senses[0]?.meaning).toBe('学校');
});

test('every sense has a translated example linked to its own stable identifier', () => {
  const childIds: string[] = [];
  demoWords.forEach(word => {
    const detail = demoDetail(word.id);
    const { senses, examples, pitches, note } = detail.content;
    expect(detail).toMatchObject({ revision: 1, created_at: 0, updated_at: word.updated_at, card_state: null, due_at: null, suspended: false, buried_until: null });
    expect(detail.content).toMatchObject({ id: word.id, kanji: word.kanji, kana: word.kana, jlpt: word.jlpt, pos: word.pos });
    expect(senses).toHaveLength(word.sense_count);
    expect(senses[0]).toEqual({ id: `sense-${word.id}`, meaning: word.meaning });
    expect(examples).toHaveLength(senses.length);
    expect(note.trim()).not.toBe('');
    senses.forEach(sense => {
      expect(sense.meaning.trim()).not.toBe('');
      expect(examples.filter(example => example.sense_id === sense.id)).toHaveLength(1);
    });
    examples.forEach(example => {
      expect(example.jp.trim()).not.toBe('');
      expect(example.cn?.trim()).toBeTruthy();
    });
    childIds.push(...[...senses, ...examples, ...pitches].map(child => child.id));
    expect(demoDetail(word.id)).toEqual(detail);
  });
  expect(new Set(childIds).size).toBe(childIds.length);
});

test.each(['demo-0', 'demo-3', 'demo-4', 'demo-6'])('%s demonstrates multiple senses with corresponding examples and notes', id => {
  expect(demoDetail(id).content.senses.length).toBeGreaterThan(1);
});

test('only the known Tokyo pitch for 食べる is supplied', () => {
  expect(demoDetail('demo-3').content.pitches).toEqual([{ id: 'pitch-demo-3-0', downstep: 2 }]);
  demoWords.filter(word => word.id !== 'demo-3').forEach(word => {
    expect(demoDetail(word.id).content.pitches).toEqual([]);
  });
});

test.each([
  ['校方', '学校'],
  ['维持生计', '食べる'],
  ['打折', '勉強'],
  ['心意', 'ありがとう'],
  ['专业人士', '先生'],
  ['茶歇', 'お茶'],
  ['宾语', '食べる'],
  ['卖方', '勉強'],
  ['前往', '学校'],
  ['更礼貌', 'ありがとう'],
])('search includes all senses and notes: %s', (text, kanji) => {
  expect(searchDemo({ ...query, text }).map(word => word.kanji)).toEqual([kanji]);
});

test.each([
  ['がっこう', '学校'],
  ['ガッコウ', '学校'],
  ['カ\u3099ッコウ', '学校'],
  ['か\u3099っこう', '学校'],
  ['ヘ\u3099ンキョウ', '勉強'],
  ['アリガトウゴザイマス', 'ありがとう'],
  ['  タベル\t谋生\n宾语  ', '食べる'],
  ['打折\u3000ベンキョウ\u3000卖方', '勉強'],
])('search normalizes NFC and kana and requires every whitespace-separated term: %s', (text, kanji) => {
  expect(searchDemo({ ...query, text }).map(word => word.kanji)).toEqual([kanji]);
});

test('search uses AND across fields, excludes examples, and handles empty text', () => {
  expect(searchDemo({ ...query, text: '学校 谋生' })).toEqual([]);
  expect(searchDemo({ ...query, text: '校方 不存在' })).toEqual([]);
  expect(searchDemo({ ...query, text: '毎朝' })).toEqual([]);
  expect(searchDemo({ ...query, text: '\t \n\u3000' })).toEqual(searchDemo(query));
});

test('search retains library and study filters alongside text matching', () => {
  expect(searchDemo({ ...query, text: '打折', library_id: 'demo-basic', pos: 'noun', jlpt: 'N5' }).map(word => word.id)).toEqual(['demo-4']);
  expect(searchDemo({ ...query, text: '打折', library_id: 'demo-life' })).toEqual([]);
  expect(searchDemo({ ...query, pos: 'ichidan_verb' }).map(word => word.id)).toEqual(['demo-3']);
  expect(searchDemo({ ...query, jlpt: 'N4' })).toEqual([]);
  expect(searchDemo({ ...query, unclassified: true })).toEqual([]);
  expect(searchDemo({ ...query, state: 'new' })).toEqual([]);
  expect(searchDemo({ ...query, due: true })).toEqual([]);
  expect(searchDemo({ ...query, due: false })).toEqual(searchDemo(query));
});

test.each(['', 'demo-8', 'demo-00', 'unknown'])('unknown detail identifier %s fails explicitly', id => {
  expect(() => demoDetail(id)).toThrow('演示词条不存在');
});
