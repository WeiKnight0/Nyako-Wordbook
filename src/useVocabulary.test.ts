import { afterEach, expect, test, vi } from 'vitest';
import { createApp, type App } from 'vue';
import { useVocabulary } from './composables/useVocabulary';
import type { WordDetail, WordPage, WordRow } from './vocabulary';

const mocks = vi.hoisted(() => ({ vocabulary: vi.fn() }));
vi.mock('./vocabulary', async original => ({ ...await original<typeof import('./vocabulary')>(), vocabulary: mocks.vocabulary }));
let app: App;
let host: HTMLDivElement;
function mount() {
  let state!: ReturnType<typeof useVocabulary>;
  host = document.createElement('div');
  app = createApp({ setup() { state = useVocabulary(); return () => null; } });
  app.mount(host);
  state.storage.value = { state: 'ready' } as NonNullable<typeof state.storage.value>;
  return state;
}
function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>(done => { resolve = done; });
  return { promise, resolve };
}
afterEach(() => { app.unmount(); vi.clearAllMocks(); vi.useRealTimers(); });

test('late detail response cannot replace a newer active word', async () => {
  const first = deferred<WordDetail>();
  const second = deferred<WordDetail>();
  mocks.vocabulary.mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise);
  const state = mount();
  const older = state.select('first');
  const newer = state.select('second');
  second.resolve({ revision: 2 } as WordDetail); await newer;
  first.resolve({ revision: 1 } as WordDetail); await older;
  expect(state.activeId.value).toBe('second');
  expect(state.detail.value?.revision).toBe(2);
});

test('changing filters clears selection and ignores an in-flight list', async () => {
  vi.useFakeTimers();
  const page = deferred<WordPage>();
  mocks.vocabulary.mockReturnValueOnce(page.promise);
  const state = mount();
  state.selected.value = [{ id: 'selected', revision: 1 }];
  const pending = state.search();
  state.query.value.text = '猫';
  state.schedule();
  page.resolve({ words: [], total: 99, next_cursor: null, query_id: 'old' });
  await pending;
  expect(state.selected.value).toEqual([]);
  expect(state.total.value).toBe(0);
  expect(state.notice.value).toContain('清空');
});

test('composition defers queries until confirmation', async () => {
  vi.useFakeTimers();
  mocks.vocabulary.mockResolvedValue({ words: [], total: 0, next_cursor: null, query_id: 'new' });
  const state = mount();
  state.beginComposition(); state.schedule();
  await vi.advanceTimersByTimeAsync(300);
  expect(mocks.vocabulary).not.toHaveBeenCalled();
  state.endComposition();
  await vi.advanceTimersByTimeAsync(200);
  expect(mocks.vocabulary).toHaveBeenCalledTimes(1);
});

test('selection refuses an oversized range without dropping existing choices', () => {
  const state = mount();
  const rows = Array.from({ length: 501 }, (_, i) => ({ id: String(i), revision: 1 } as WordRow));
  state.range(rows.slice(0, 500));
  state.toggle(rows[500]!);
  expect(state.selected.value).toHaveLength(500);
  expect(state.notice.value).toContain('500');
  state.toggle(rows[0]!);
  state.toggle(rows[500]!);
  expect(state.selected.value).toHaveLength(500);
  expect(state.selected.value.some(item => item.id === '500')).toBe(true);
});

test('removing the active row selects its next neighbour', async () => {
  const state = mount();
  state.words.value = ['a', 'b', 'c'].map(id => ({ id, revision: 1 } as WordRow));
  state.activeId.value = 'b';
  mocks.vocabulary.mockImplementation(request => Promise.resolve(request.operation === 'list_words'
    ? { words: [state.words.value[0], state.words.value[2]], total: 2, next_cursor: null }
    : { revision: 1 }));
  await state.search();
  expect(state.activeId.value).toBe('c');
});
