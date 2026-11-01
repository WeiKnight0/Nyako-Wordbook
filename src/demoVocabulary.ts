import type { Library, WordDetail, WordQuery, WordRow } from './vocabulary';

export const demoLibraries: Library[] = [
  { id: 'demo-basic', name: '标日初级 · 演示', description: '仅供浏览器界面预览，不在正式词库中。', archived: false, revision: 1, word_count: 5 },
  { id: 'demo-life', name: '日常表达 · 演示', description: '浏览器临时样例', archived: false, revision: 1, word_count: 3 },
];

const entries = [
  ['学校', 'がっこう', '学校', 'N5', 'noun', 'demo-basic'],
  ['先生', 'せんせい', '老师', 'N5', 'noun', 'demo-basic'],
  ['日本語', 'にほんご', '日语', 'N5', 'noun', 'demo-basic'],
  ['食べる', 'たべる', '吃', 'N5', 'ichidan_verb', 'demo-basic'],
  ['勉強', 'べんきょう', '学习', 'N5', 'noun', 'demo-basic'],
  ['おはよう', 'おはよう', '早上好', 'N5', 'expression', 'demo-life'],
  ['ありがとう', 'ありがとう', '谢谢', 'N5', 'expression', 'demo-life'],
  ['お茶', 'おちゃ', '茶', 'N5', 'noun', 'demo-life'],
] as const;

export const demoWords: WordRow[] = entries.map(([kanji, kana, meaning, jlpt, pos], index) => ({
  id: `demo-${index}`, kanji, kana, meaning, sense_count: 1, revision: 1,
  updated_at: index, jlpt, pos: [pos],
  card_state: null, due_at: null, suspended: false, buried_until: null,
}));

/** @description 从隔离样例生成只读词条详情。 @param id 演示词条标识。 @throws 未知标识时报错。 */
export function demoDetail(id: string): WordDetail {
  const word = demoWords.find(item => item.id === id);
  if (!word) throw new Error('演示词条不存在');
  const library = entries[Number(id.slice(5))]![5];
  return {
    content: { id, kanji: word.kanji, kana: word.kana, jlpt: word.jlpt, note: '',
      senses: [{ id: `sense-${id}`, meaning: word.meaning }], examples: [], pitches: [],
      pos: word.pos, library_ids: [library] },
    revision: 1, created_at: 0, updated_at: word.updated_at, card_state: null,
    due_at: null, suspended: false, buried_until: null,
  };
}

/** @description 在浏览器演示中应用与正式界面相同的检索条件。 @param query 当前筛选条件。 @throws 无。 */
export function searchDemo(query: WordQuery): WordRow[] {
  const text = query.text.trim().normalize('NFC').toLocaleLowerCase();
  const matching = demoWords.filter(word => {
    const library = entries[Number(word.id.slice(5))]![5];
    if (query.library_id && library !== query.library_id) return false;
    if (query.unclassified) return false;
    if (text && ![word.kanji, word.kana, word.meaning].some(value => value?.toLocaleLowerCase().includes(text))) return false;
    if (query.jlpt && word.jlpt !== query.jlpt) return false;
    if (query.pos && !word.pos.includes(query.pos)) return false;
    if (query.state && word.card_state !== query.state) return false;
    if (query.due === true) return false;
    return true;
  });
  return matching.sort((left, right) => {
    const comparison = query.sort === 'kanji' || query.sort === 'kana'
      ? (left[query.sort] || '').localeCompare(right[query.sort] || '', 'ja')
      : left.updated_at - right.updated_at;
    return query.descending ? -comparison : comparison;
  });
}
