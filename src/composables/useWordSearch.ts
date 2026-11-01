import { computed, onBeforeUnmount, ref, type Ref } from 'vue';
import { api, desktop, errorMessage, type Word } from '../api';

/**
 * @description 管理检索时序、输入法组合输入及虚拟列表选择。
 * @param error 页面向用户显示的错误状态。
 * @throws 异步查询失败在当前请求入口转换为可见错误。
 */
export function useWordSearch(error: Ref<string>) {
  const words = ref<Word[]>([]);
  const query = ref('');
  const loading = ref(false);
  const cursor = ref<number | null>(null);
  const elapsed = ref(0);
  const active = ref(0);
  const scrollTop = ref(0);
  const viewport = ref<HTMLElement>();
  const composing = ref(false);
  const rowHeight = 60;
  const start = computed(() => Math.max(0, Math.floor(scrollTop.value / rowHeight) - 4));
  const visible = computed(() => words.value.slice(start.value, start.value + 24));
  const selected = computed(() => words.value[active.value]);
  let sequence = 0;
  let timer: ReturnType<typeof setTimeout> | undefined;

  async function search(append = false) {
    if (!desktop) return;
    const request = ++sequence;
    loading.value = true;
    error.value = '';
    try {
      const result = await api.search(query.value, append ? cursor.value ?? 0 : 0);
      if (request !== sequence) return;
      words.value = append ? [...words.value, ...result.words] : result.words;
      cursor.value = result.next_cursor;
      elapsed.value = result.elapsed_ms;
      if (append) return;
      active.value = 0;
      scrollTop.value = 0;
      if (viewport.value) viewport.value.scrollTop = 0;
    } catch (cause) {
      if (request === sequence) error.value = errorMessage(cause);
    } finally {
      if (request === sequence) loading.value = false;
    }
  }

  function beginComposition() {
    composing.value = true;
    clearTimeout(timer);
    ++sequence;
  }

  function scheduleSearch() {
    if (composing.value) return;
    ++sequence;
    clearTimeout(timer);
    timer = setTimeout(() => void search(), 200);
  }

  function endComposition() {
    composing.value = false;
    scheduleSearch();
  }

  function select(index: number) {
    active.value = index;
    viewport.value?.focus();
  }

  function scroll(event: Event) {
    scrollTop.value = (event.target as HTMLElement).scrollTop;
  }

  function navigate(event: KeyboardEvent) {
    if (event.isComposing || event.repeat || !['ArrowDown', 'ArrowUp', 'j', 'k'].includes(event.key)) return;
    event.preventDefault();
    active.value = Math.max(0, Math.min(words.value.length - 1, active.value + (['ArrowDown', 'j'].includes(event.key) ? 1 : -1)));
    const top = active.value * rowHeight;
    if (!viewport.value) return;
    if (top < viewport.value.scrollTop || top + rowHeight > viewport.value.scrollTop + viewport.value.clientHeight) {
      viewport.value.scrollTop = top;
    }
  }

  onBeforeUnmount(() => {
    clearTimeout(timer);
    ++sequence;
  });

  return {
    words, query, loading, cursor, elapsed, active, scrollTop, viewport,
    rowHeight, start, visible, selected, search, beginComposition,
    endComposition, scheduleSearch, select, scroll, navigate,
  };
}
