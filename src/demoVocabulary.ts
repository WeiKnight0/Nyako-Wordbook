import type { Library, WordDetail, WordQuery, WordRow } from './vocabulary';

export const demoLibraries: Library[] = [
  { id: 'demo-basic', name: '标日初级 · 演示', description: '仅供浏览器界面预览，不在正式词库中。', archived: false, revision: 1, word_count: 5 },
  { id: 'demo-life', name: '日常表达 · 演示', description: '浏览器临时样例', archived: false, revision: 1, word_count: 3 },
];

const entries = [
  {
    kanji: '学校', kana: 'がっこう', pos: 'noun', library: 'demo-basic', pitch: '',
    note: '「学校に通う」表示上学；「学校へ行く」强调前往学校。也可用「学校」指校方或学校这一组织。',
    meaning: '学校\n\n校方；学校这一组织',
    examples_jp: '妹は毎日、歩いて学校に通っています。\n\n学校から保護者に連絡がありました。',
    examples_cn: '妹妹每天步行上学。\n\n校方向家长发来了通知。',
  },
  {
    kanji: '先生', kana: 'せんせい', pos: 'noun', library: 'demo-basic', pitch: '',
    note: '可用于称呼老师，也可用于医生、律师等专业人士。称呼自己时一般不用「先生」。',
    meaning: '老师\n\n先生；对医生等专业人士的敬称',
    examples_jp: '先生に日本語の発音を教えてもらいました。\n\n先生、この薬はいつ飲めばいいですか。',
    examples_cn: '老师教了我日语发音。\n\n医生，这个药应该什么时候吃？',
  },
  {
    kanji: '日本語', kana: 'にほんご', pos: 'noun', library: 'demo-basic', pitch: '',
    note: '「日本語で」表示使用日语；「日本語を勉強する」表示学习日语。',
    meaning: '日语',
    examples_jp: 'この言葉は日本語で何と言いますか。',
    examples_cn: '这个词用日语怎么说？',
  },
  {
    kanji: '食べる', kana: 'たべる', pos: 'ichidan_verb', library: 'demo-basic', pitch: '2',
    note: '一段他动词，宾语通常用「を」。东京式音调为②；「食べていく」还可表示维持生计。',
    meaning: '吃\n\n谋生；维持生计',
    examples_jp: '毎朝、パンと卵を食べます。\n\n将来は絵を描いて食べていきたいです。',
    examples_cn: '我每天早上吃面包和鸡蛋。\n\n将来我想靠画画谋生。',
  },
  {
    kanji: '勉強', kana: 'べんきょう', pos: 'noun', library: 'demo-basic', pitch: '',
    note: '常与「する」搭配，表示学习。买卖场合的「勉強する」可表示让价，通常由卖方使用。',
    meaning: '学习\n\n让价；打折（买卖用语）',
    examples_jp: '毎晩、一時間日本語を勉強しています。\n\n二つ買ってくださるなら、少し勉強します。',
    examples_cn: '我每天晚上学习一个小时日语。\n\n如果您买两个，我就给您便宜一点。',
  },
  {
    kanji: 'おはよう', kana: 'おはよう', pos: 'expression', library: 'demo-life', pitch: '',
    note: '用于家人、朋友等熟人之间。对不熟悉的人或在正式场合，一般说「おはようございます」。',
    meaning: '早上好',
    examples_jp: 'おはよう。よく眠れた？',
    examples_cn: '早上好。睡得好吗？',
  },
  {
    kanji: 'ありがとう', kana: 'ありがとう', pos: 'expression', library: 'demo-life', pitch: '',
    note: '本例按感谢的使用场景分项。「〜てくれてありがとう」感谢对方做了某事；更礼貌的表达是「ありがとうございます」。',
    meaning: '谢谢\n\n感谢对方的帮助或心意（〜てくれてありがとう）',
    examples_jp: '素敵なプレゼントをありがとう。\n\n忙しいのに、手伝ってくれてありがとう。',
    examples_cn: '谢谢你送我这么棒的礼物。\n\n你这么忙还来帮我，谢谢你。',
  },
  {
    kanji: 'お茶', kana: 'おちゃ', pos: 'noun', library: 'demo-life', pitch: '',
    note: '「お」是常用的礼貌前缀。「お茶にする」可表示停下来喝茶、稍作休息。',
    meaning: '茶\n\n喝茶休息；茶歇',
    examples_jp: '温かいお茶を一杯ください。\n\n少し休んで、お茶にしましょう。',
    examples_cn: '请给我一杯热茶。\n\n稍微休息一下，喝点茶吧。',
  },
];

export const demoWords: WordRow[] = entries.map(({ kanji, kana, meaning, pos }, index) => ({
  id: `demo-${index}`, kanji, kana, meaning: meaning.split(/\n/)[0]!, revision: 1,
  updated_at: index, jlpt: 'N5', pos: [pos],
  card_state: null, due_at: null, suspended: false, buried_until: null,
}));

/** @description 从隔离样例生成只读词条详情。 @param id 演示词条标识。 @throws 未知标识时报错。 */
export function demoDetail(id: string): WordDetail {
  const index = demoWords.findIndex(word => word.id === id);
  if (index === -1) throw new Error('演示词条不存在');
  const word = demoWords[index]!;
  const { library, meaning, pitch, examples_jp, examples_cn, note } = entries[index]!;
  return {
    content: { id, kanji: word.kanji, kana: word.kana, jlpt: word.jlpt, note,
      meaning, pitch, examples_jp, examples_cn,
      pos: word.pos, library_ids: [library] },
    revision: 1, created_at: 0, updated_at: word.updated_at, card_state: null,
    due_at: null, suspended: false, buried_until: null,
  };
}

function normalize(text: string): string {
  return text.normalize('NFC').toLowerCase().replace(/[ァ-ヶ]/g, char => String.fromCharCode(char.charCodeAt(0) - 0x60));
}

/** @description 在浏览器演示中应用与正式界面相同的检索条件。 @param query 当前筛选条件。 @throws 无。 */
export function searchDemo(query: WordQuery): WordRow[] {
  const terms = query.text.trim().split(/\s+/u).filter(Boolean).map(normalize);
  const matching = demoWords.filter((word, index) => {
    const { library, meaning, note } = entries[index]!;
    if (query.library_id && library !== query.library_id) return false;
    if (query.unclassified) return false;
    const text = [word.kanji, word.kana, meaning, note].map(normalize).join('\n');
    if (!terms.every(term => text.includes(term))) return false;
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
