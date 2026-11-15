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
  const detailLoading = ref(false);
  const detailError = ref('');
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

  function activate(id: string) {
    if (activeId.value === id) return;
    ++detailSequence;
    activeId.value = id;
    detail.value = undefined;
    detailLoading.value = false;
    detailError.value = '';
  }

  async function select(id: string) {
    if (id === activeId.value && (detailLoading.value || detail.value)) return;
    activate(id);
    detailError.value = '';
    detailLoading.value = true;
    const request = ++detailSequence;
    try {
      const result = demo ? demoDetail(id) : await vocabulary<WordDetail>({ operation: 'get_word', id });
      if (request === detailSequence) detail.value = result;
    } catch (cause) {
      if (request === detailSequence) { detail.value = undefined; detailError.value = errorMessage(cause); }
    } finally { if (request === detailSequence) detailLoading.value = false; }
  }

  async function search(append = false, preserve = false): Promise<boolean> {
    if (!ready.value) return false;
    const request = ++sequence;
    const snapshot = JSON.parse(JSON.stringify(query.value)) as WordQuery;
    const target = preserve ? words.value.length : 0;
    loading.value = true;
    error.value = '';
    detailError.value = '';
    try {
      let next = append ? cursor.value : null;
      let rows: WordRow[] = [];
      let count = 0;
      do {
        const page: WordPage = demo
          ? { words: searchDemo(snapshot), next_cursor: null, total: searchDemo(snapshot).length, query_id: 'demo' }
          : await vocabulary<WordPage>({ operation: 'list_words', query: snapshot, cursor: next, limit: 100 });
        if (request !== sequence) return false;
        rows = [...rows, ...page.words];
        count = page.total;
        next = page.next_cursor;
      } while (next && rows.length < target);
      const oldIndex = Math.max(0, words.value.findIndex(word => word.id === activeId.value));
      words.value = [...new Map([...(append ? words.value : []), ...rows].map(word => [word.id, word])).values()];
      total.value = count;
      cursor.value = next;
      if (append) return true;
      ++detailSequence;
      detail.value = undefined;
      detailLoading.value = false;
      activeId.value = rows.find(word => word.id === activeId.value)?.id ?? rows[Math.min(oldIndex, rows.length - 1)]?.id ?? '';
      if (preserve) {
        const revisions = new Map(rows.map(word => [word.id, word.revision]));
        selected.value = selected.value.filter(word => revisions.has(word.id)).map(word => ({ id: word.id, revision: revisions.get(word.id)! }));
      }
      return true;
    } catch (cause) { if (request === sequence) error.value = errorMessage(cause); return false; }
    finally { if (request === sequence) loading.value = false; }
  }

  function invalidate() {
    ++sequence;
    ++detailSequence;
    detailLoading.value = false;
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

  async function refresh(preserve = false): Promise<boolean> {
    const request = ++sequence;
    ++detailSequence;
    detail.value = undefined;
    detailLoading.value = false;
    clearTimeout(timer);
    const result = demo ? demoLibraries : await vocabulary<Library[]>({ operation: 'libraries' });
    if (request !== sequence) return false;
    libraries.value = result;
    if (query.value.library_id && !libraries.value.some(library => library.id === query.value.library_id)) query.value.library_id = null;
    return search(false, preserve);
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
  return { libraries, storage, error, notice, loading, words, detail, detailLoading, detailError, activeId, selected, total, cursor, query, ready, activate, select, search, schedule, beginComposition, endComposition, toggle, range, refresh, initialize };
}
