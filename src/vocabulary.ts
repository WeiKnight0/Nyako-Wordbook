import { invoke } from '@tauri-apps/api/core';

/** @description 正式词库。 @param 不适用。 @throws 不适用。 */
export interface Library { id: string; name: string; description: string; archived: boolean; revision: number; word_count: number }
/** @description 稳定义项。 @param 不适用。 @throws 不适用。 */
export interface Sense { id: string; meaning: string }
/** @description 例句与可选义项关联。 @param 不适用。 @throws 不适用。 */
export interface Example { id: string; sense_id: string | null; jp: string; cn: string | null }
/** @description 音调位置。 @param 不适用。 @throws 不适用。 */
export interface Pitch { id: string; downstep: number }
/** @description 原子保存的词条内容。 @param 不适用。 @throws 不适用。 */
export interface WordInput {
  id: string; kanji: string; kana: string; jlpt: string | null; note: string;
  senses: Sense[]; examples: Example[]; pitches: Pitch[]; pos: string[]; library_ids: string[];
}
/** @description 词条内容与版本。 @param 不适用。 @throws 不适用。 */
export interface WordDetail {
  content: WordInput; revision: number; created_at: number; updated_at: number;
  card_state: string | null; due_at: number | null; suspended: boolean; buried_until: number | null;
}
/** @description 正式列表投影。 @param 不适用。 @throws 不适用。 */
export interface WordRow {
  id: string; kanji: string; kana: string; meaning: string; sense_count: number;
  revision: number; updated_at: number; jlpt: string | null; pos: string[];
  card_state: string | null; due_at: number | null; suspended: boolean; buried_until: number | null;
}
/** @description 正式检索条件。 @param 不适用。 @throws 不适用。 */
export interface WordQuery {
  text: string; library_id: string | null; unclassified: boolean;
  jlpt: string | null; pos: string | null; state: string | null; due: boolean | null;
  sort: string; descending: boolean;
}
/** @description 游标查询结果。 @param 不适用。 @throws 不适用。 */
export interface WordPage { words: WordRow[]; next_cursor: string | null; total: number; query_id: string }
/** @description 显式操作范围。 @param 不适用。 @throws 不适用。 */
export interface SelectedWord { id: string; revision: number }
/** @description 永久删除影响。 @param 不适用。 @throws 不适用。 */
export interface DeleteImpact { memberships: number; cards: number; events: number }

type Request =
  | { operation: 'libraries' }
  | { operation: 'save_library'; library: Library; expected: number | null }
  | { operation: 'delete_library'; id: string; revision: number }
  | { operation: 'get_word'; id: string }
  | { operation: 'save_word'; content: WordInput; expected: number | null }
  | { operation: 'list_words'; query: WordQuery; cursor: string | null; limit: number }
  | { operation: 'delete_impact'; selected: SelectedWord[] }
  | { operation: 'batch'; selected: SelectedWord[]; action: string; target: string | null };

/** @description 调用正式库类型化命令。 @param request 业务操作。 @throws 后端拒绝时 Promise 失败。 */
export function vocabulary<T>(request: Request): Promise<T> { return invoke<T>('vocabulary', { request }); }

/** @description 首批稳定词性编码及显示名。 @param 不适用。 @throws 不适用。 */
export const partsOfSpeech = [
  ['noun', '名词'], ['godan_verb', '五段动词'], ['ichidan_verb', '一段动词'],
  ['suru_verb', 'サ变动词'], ['kuru_verb', 'カ变动词'], ['i_adjective', 'い形容词'],
  ['na_adjective', 'な形容词'], ['adverb', '副词'], ['particle', '助词'],
  ['expression', '表达'], ['other', '其他'], ['intransitive', '自动词'], ['transitive', '他动词'],
] as const;

export const verbForms = partsOfSpeech.filter(([code]) => ['godan_verb', 'ichidan_verb', 'suru_verb', 'kuru_verb'].includes(code));
export const otherPartsOfSpeech = partsOfSpeech.filter(([code]) => ![...verbForms.map(([form]) => form), 'intransitive', 'transitive'].includes(code));
export function posLabels(pos: string[]): string { return partsOfSpeech.filter(([code]) => pos.includes(code)).map(([, label]) => label).join(' / '); }

/** @description 为用户录入提供可修正的日语拍数提示。 @param kana 单个读音。 @throws 无。 */
export function moraHint(kana: string): number {
  return Array.from(kana.normalize('NFC')).filter(char => !/[ゃゅょぁぃぅぇぉャュョァィゥェォ\s]/.test(char)).length;
}
