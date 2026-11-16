import { afterEach, beforeEach, expect, test, vi } from 'vitest';
import { createApp, defineComponent, h, KeepAlive, nextTick, ref, type App } from 'vue';
import AppView from './App.vue';
import LibraryView from './views/LibraryView.vue';
import type { WordDetail, WordRow } from './vocabulary';

const mocks = vi.hoisted(() => ({ vocabulary: vi.fn(), closeHandler: undefined as undefined | ((event: { preventDefault: () => void }) => void) }));
vi.mock('./api', () => ({ desktop: true, api: { storage: async () => ({ state: 'ready' }), health: async () => ({ acceptance_measure: false }) }, errorMessage: String }));
vi.mock('./vocabulary', async original => ({ ...await original<typeof import('./vocabulary')>(), vocabulary: mocks.vocabulary }));
vi.mock('@tauri-apps/api/window', () => ({ getCurrentWindow: () => ({ onCloseRequested: async (handler: typeof mocks.closeHandler) => { mocks.closeHandler = handler; return () => {}; } }) }));
vi.mock('./views/DiagnosticsView.vue', () => ({ default: { template: '<p>诊断页</p>' } }));
vi.mock('./views/DataView.vue', () => ({ default: defineComponent({
  props: ['selected', 'entrySection', 'entry'], emits: ['back', 'changed'],
  setup: (props, { emit }) => () => h('main', { class: 'data-test' }, [
    h('p', `${props.entrySection}:${props.selected.join(',')}`),
    h('button', { onClick: () => emit('back') }, '返回词库'),
    h('button', { onClick: () => emit('changed') }, '写入完成'),
  ]),
}) }));
let app: App;
let host: HTMLDivElement;
let revision: number;
let rows: WordRow[];
async function settle() { for (let i = 0; i < 30; i++) await Promise.resolve(); await nextTick(); }
async function transition() { await vi.advanceTimersByTimeAsync(100); await settle(); }
function button(text: string) { return [...host.querySelectorAll<HTMLButtonElement>('button')].find(item => item.textContent?.trim() === text)!; }
function mount(component: Parameters<typeof createApp>[0] = AppView) { app = createApp(component); app.mount(host); }
function queries() { return mocks.vocabulary.mock.calls.filter(([request]) => request.operation === 'list_words'); }
function key(target: Element, value: string, options: KeyboardEventInit = {}) { target.dispatchEvent(new KeyboardEvent('keydown', { key: value, bubbles: true, cancelable: true, ...options })); }
function detail(id: string): WordDetail { return { content: { id, kanji: `词${id}`, kana: id, pos: [], pitches: [], senses: [{ id: 'sense', meaning: `释义${id}` }], examples: [], library_ids: [], note: '', jlpt: null }, revision, created_at: 0, updated_at: 0, card_state: null, due_at: null, suspended: false, buried_until: null }; }
beforeEach(async () => {
  vi.useFakeTimers();
  revision = 1;
  rows = Array.from({ length: 150 }, (_, index) => ({ id: String(index), kanji: `词${index}`, kana: String(index), meaning: `释义${index}`, pos: [], sense_count: 1, revision: 1, updated_at: 0, jlpt: null, card_state: null, due_at: null, suspended: false, buried_until: null }));
  mocks.vocabulary.mockReset().mockImplementation(request => {
    if (request.operation === 'libraries') return Promise.resolve([{ id: 'library', name: '常用', description: '', archived: false, revision, word_count: rows.length }]);
    if (request.operation === 'get_word') return Promise.resolve(detail(request.id));
    if (request.operation === 'list_words') {
      const offset = Number(request.cursor ?? 0);
      return Promise.resolve({ words: rows.slice(offset, offset + 100), total: rows.length, next_cursor: offset + 100 < rows.length ? String(offset + 100) : null });
    }
    return Promise.resolve(1);
  });
  host = document.createElement('div'); document.body.append(host);
  mount(); await settle();
});
afterEach(() => { app.unmount(); host.remove(); vi.useRealTimers(); });

test('data and diagnostics round trips preserve query, pagination, checkboxes and scroll without stale cache reads', async () => {
  button('常用 150').click(); await vi.advanceTimersByTimeAsync(200); await settle();
  const input = host.querySelector<HTMLInputElement>('[aria-label="搜索真实词条"]')!;
  input.value = '猫'; input.dispatchEvent(new Event('input', { bubbles: true }));
  await vi.advanceTimersByTimeAsync(200); await settle();
  host.querySelector<HTMLInputElement>('[aria-label="选择 词0"]')!.click(); await settle();
  button('下一页').click(); await settle();
  const list = host.querySelector<HTMLElement>('[role="listbox"]')!;
  list.scrollTop = 7200; list.dispatchEvent(new Event('scroll')); await settle();
  host.querySelector<HTMLInputElement>('[aria-label="选择 词105"]')!.click(); await settle();
  const calls = queries().length;
  button('导出所选词条').click(); await transition();
  expect(host.querySelector('.data-test')?.textContent).toContain('export:0,105');
  const inactiveShortcut = new KeyboardEvent('keydown', { key: 'n', ctrlKey: true, cancelable: true });
  window.dispatchEvent(inactiveShortcut);
  expect(inactiveShortcut.defaultPrevented).toBe(false);
  button('返回词库').click(); await transition();
  expect(host.querySelector('[role="listbox"]')).toBe(list);
  expect(input.value).toBe('猫');
  expect(list.scrollTop).toBe(7200);
  expect(host.textContent).toContain('已加载 150 / 150 条');
  expect(host.textContent).toContain('当前已选 2 条');
  expect(host.querySelector<HTMLInputElement>('[aria-label="选择 词105"]')!.checked).toBe(true);
  button('技术诊断').click(); await transition();
  expect(host.textContent).toContain('诊断页');
  button('返回真实词库').click(); await transition();
  expect(host.querySelector('[role="listbox"]')).toBe(list);
  expect(list.scrollTop).toBe(7200);
  expect(queries()).toHaveLength(calls);
  expect(queries().at(-1)![0].query).toMatchObject({ text: '猫', library_id: 'library' });
  expect(host.querySelector('[role="dialog"]')).toBeNull();
});

test('pagination returns to cached pages and disables both result boundaries', async () => {
  button('列表').click(); await settle();
  expect(button('上一页').disabled).toBe(true);
  expect(host.textContent).toContain('第 1 / 2 页');
  button('下一页').click(); await settle();
  expect(host.querySelector('[role="listbox"]')?.getAttribute('aria-activedescendant')).toBe('formal-100');
  expect(host.textContent).toContain('第 2 / 2 页');
  expect(button('下一页').disabled).toBe(true);
  const calls = queries().length;
  button('上一页').click(); await settle();
  expect(host.querySelector('[role="listbox"]')?.getAttribute('aria-activedescendant')).toBe('formal-0');
  expect(host.querySelector<HTMLElement>('[role="listbox"]')!.scrollTop).toBe(0);
  expect(host.textContent).toContain('第 1 / 2 页');
  expect(button('上一页').disabled).toBe(true);
  button('下一页').click(); await settle();
  expect(queries()).toHaveLength(calls);
  expect(host.textContent).toContain('第 2 / 2 页');
});

test('failed next page keeps the current page and allows retrying', async () => {
  mocks.vocabulary.mockRejectedValueOnce(new Error('page failed'));
  button('下一页').click(); await settle();
  expect(host.querySelector('[role="alert"]')?.textContent).toContain('page failed');
  expect(host.textContent).toContain('第 1 / 2 页');
  expect(button('下一页').disabled).toBe(false);
  button('下一页').click(); await settle();
  expect(host.textContent).toContain('第 2 / 2 页');
});

test('changed data reloads loaded pages and batch operations use current selection revisions', async () => {
  host.querySelector<HTMLInputElement>('[aria-label="选择 词0"]')!.click(); await settle();
  button('下一页').click(); await settle();
  const list = host.querySelector<HTMLElement>('[role="listbox"]')!;
  list.scrollTop = 7200; list.dispatchEvent(new Event('scroll')); await settle();
  host.querySelector<HTMLInputElement>('[aria-label="选择 词105"]')!.click(); await settle();
  button('数据管理').click(); await transition();
  revision = 7;
  rows = rows.filter(row => row.id !== '0').map(row => ({ ...row, revision, meaning: `新释义${row.id}` }));
  const calls = queries().length;
  button('写入完成').click(); await settle();
  expect(queries()).toHaveLength(calls);
  button('返回词库').click(); await transition();
  expect(queries()).toHaveLength(calls + 2);
  expect(host.textContent).toContain('当前已选 1 条');
  expect(host.textContent).toContain('已加载 149 / 149 条');
  expect(list.scrollTop).toBe(99 * 72);
  expect(list.getAttribute('aria-activedescendant')).toBe('formal-100');
  list.scrollTop = 7200; list.dispatchEvent(new Event('scroll')); await settle();
  expect(host.textContent).toContain('新释义105');
  const target = host.querySelector<HTMLSelectElement>('[aria-label="目标词库"]')!;
  target.value = 'library'; target.dispatchEvent(new Event('change', { bubbles: true })); await settle();
  button('加入词库').click(); await settle();
  expect(mocks.vocabulary).toHaveBeenCalledWith({ operation: 'batch', selected: [{ id: '105', revision: 7 }], action: 'add_library', target: 'library' });
});

test('failed refresh blocks stale selection actions and can retry on the same page', async () => {
  host.querySelector<HTMLInputElement>('[aria-label="选择 词0"]')!.click(); await settle();
  button('数据管理').click(); await transition();
  button('写入完成').click(); await settle();
  mocks.vocabulary.mockRejectedValueOnce(new Error('refresh failed'));
  button('返回词库').click(); await transition();
  expect(host.querySelector('[role="alert"]')?.textContent).toContain('refresh failed');
  expect(button('永久删除词条…').disabled).toBe(true);
  expect(button('导出所选词条').disabled).toBe(true);
  rows = rows.map(row => ({ ...row, revision: 9 }));
  button('重试刷新').click(); await settle();
  expect(button('永久删除词条…').disabled).toBe(false);
  expect(button('重试刷新')).toBeUndefined();
});

test('cached library neither handles global shortcuts nor intercepts window close while inactive', async () => {
  app.unmount();
  const visible = ref(true);
  mount({ setup: () => () => h(KeepAlive, () => visible.value ? h(LibraryView) : null) }); await settle();
  button('新建词库').click(); await settle();
  const field = host.querySelector<HTMLInputElement>('[role="dialog"] input')!;
  field.value = '草稿'; field.dispatchEvent(new Event('input', { bubbles: true })); await settle();
  visible.value = false; await settle();
  const preventDefault = vi.fn();
  mocks.closeHandler!({ preventDefault });
  const shortcut = new KeyboardEvent('keydown', { key: 'f', ctrlKey: true, cancelable: true });
  window.dispatchEvent(shortcut); await settle();
  expect(preventDefault).not.toHaveBeenCalled();
  expect(shortcut.defaultPrevented).toBe(false);
  visible.value = true; await settle();
  expect(button('保存并离开')).toBeUndefined();
  mocks.closeHandler!({ preventDefault }); await settle();
  expect(preventDefault).toHaveBeenCalledOnce();
  expect(button('保存并离开')).toBeTruthy();
});
