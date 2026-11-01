import { computed, onBeforeUnmount, ref } from 'vue';
import { errorMessage, api, desktop, type StorageStatus } from '../api';
import { demoDetail, demoLibraries, searchDemo } from '../demoVocabulary';
import { vocabulary, type Library, type WordDetail, type WordPage, type WordQuery, type WordRow, type SelectedWord } from '../vocabulary';

/** @description 管理真实词库查询与活动详情，隔离过期响应。 @param 无。 @throws 业务错误由事件入口展示。 */
export function useVocabulary() {
  const libraries = ref<Library[]>([]);
  const storage = ref<StorageStatus>();
  const error = ref('');
  const notice = ref('');
  const loading = ref(false);
  const words = ref<WordRow[]>([]);
  const detail = ref<WordDetail>();
  const activeId = ref('');
  const selected = ref<SelectedWord[]>([]);
  const total = ref(0);
  const cursor = ref<string | null>(null);
  const query = ref<WordQuery>({ text: '', library_id: null, unclassified: false, jlpt: null, pos: null, state: null, due: null, sort: 'updated_at', descending: true });
  const demo = import.meta.env.DEV && !desktop;
  const ready = computed(() => demo || storage.value?.state === 'ready');
  let sequence = 0;
  let detailSequence = 0;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let composing = false;

  async function select(id: string) {
    activeId.value = id;
    detail.value = undefined;
    const request = ++detailSequence;
    try {
      const result = demo ? demoDetail(id) : await vocabulary<WordDetail>({ operation: 'get_word', id });
      if (request === detailSequence) detail.value = result;
    } catch (cause) { if (request === detailSequence) error.value = errorMessage(cause); }
  }

  async function search(append = false) {
    if (!ready.value) return;
    const request = ++sequence;
    loading.value = true;
    error.value = '';
    try {
      const page = demo
        ? { words: searchDemo(query.value), next_cursor: null }
        : await vocabulary<WordPage>({ operation: 'list_words', query: JSON.parse(JSON.stringify(query.value)) as WordQuery, cursor: append ? cursor.value : null, limit: 100 });
      if (request !== sequence) return;
      const wordsBefore = words.value;
      words.value = append ? [...new Map([...words.value, ...page.words].map(word => [word.id, word])).values()] : page.words;
      total.value = demo ? page.words.length : (page as WordPage).total;
      cursor.value = page.next_cursor;
      if (!append) {
        ++detailSequence;
        const oldIndex = Math.max(0, wordsBefore.findIndex(word => word.id === activeId.value));
        const id = page.words.find(word => word.id === activeId.value)?.id ?? page.words[Math.min(oldIndex, page.words.length - 1)]?.id;
        if (id) await select(id);
        else { activeId.value = ''; detail.value = undefined; }
      }
    } catch (cause) { if (request === sequence) error.value = errorMessage(cause); }
    finally { if (request === sequence) loading.value = false; }
  }

  function invalidate() {
    ++sequence;
    ++detailSequence;
    clearTimeout(timer);
    if (selected.value.length) notice.value = '查询条件已变化，已清空原选择。';
    selected.value = [];
  }

  function schedule() {
    invalidate();
    if (!composing) timer = setTimeout(() => void search(), 200);
  }
  function beginComposition() { composing = true; invalidate(); }
  function endComposition() { composing = false; schedule(); }
  function range(rows: WordRow[]) {
    const next = [...new Map([...selected.value, ...rows.map(word => ({ id: word.id, revision: word.revision }))].map(word => [word.id, word])).values()];
    if (next.length > 500) { notice.value = '一次最多选择 500 条，请分批操作；原选择已保留。'; return; }
    selected.value = next;
  }
  function toggle(word: WordRow) {
    if (selected.value.some(item => item.id === word.id)) {
      selected.value = selected.value.filter(item => item.id !== word.id);
      return;
    }
    range([word]);
  }

  async function refresh() {
    libraries.value = demo ? demoLibraries : await vocabulary<Library[]>({ operation: 'libraries' });
    await search();
  }

  let startupTimer: ReturnType<typeof setTimeout> | undefined;
  async function initialize() {
    if (demo) { await refresh(); return; }
    try {
      storage.value = await api.storage();
      if (storage.value.state === 'ready') await refresh();
      else if (storage.value.state === 'initializing') startupTimer = setTimeout(() => void initialize(), 250);
    } catch (cause) { error.value = errorMessage(cause); }
  }

  onBeforeUnmount(() => { clearTimeout(timer); clearTimeout(startupTimer); ++sequence; ++detailSequence; });
  return { libraries, storage, error, notice, loading, words, detail, activeId, selected, total, cursor, query, ready, select, search, schedule, beginComposition, endComposition, toggle, range, refresh, initialize };
}
