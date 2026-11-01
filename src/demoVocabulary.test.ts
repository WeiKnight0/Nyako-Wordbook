import { expect, test } from 'vitest';
import { demoDetail, demoLibraries, demoWords, searchDemo } from './demoVocabulary';
import type { WordQuery } from './vocabulary';

const query: WordQuery = { text: '', library_id: null, unclassified: false, jlpt: null, pos: null, state: null, due: null, sort: 'updated_at', descending: true };

test('browser samples can be searched and scoped without changing formal data', () => {
  expect(demoWords).toHaveLength(8);
  expect(searchDemo(query)).toHaveLength(8);
  expect(searchDemo({ ...query, library_id: demoLibraries[0]!.id })).toHaveLength(5);
  expect(searchDemo({ ...query, text: 'がっこう' }).map(word => word.kanji)).toEqual(['学校']);
  expect(searchDemo({ ...query, unclassified: true })).toEqual([]);
  expect(demoDetail('demo-0').content.senses[0]?.meaning).toBe('学校');
});
