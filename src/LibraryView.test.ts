import { afterEach, beforeEach, expect, test, vi } from 'vitest';
import { createApp, nextTick, type App } from 'vue';
import LibraryView from './views/LibraryView.vue';

const mocks = vi.hoisted(() => ({ vocabulary: vi.fn(), destroy: vi.fn(), closeHandler: undefined as undefined | ((event: { preventDefault: () => void }) => void) }));
vi.mock('./api', () => ({ desktop: true, api: { storage: async () => ({ state: 'ready' }), health: async () => ({ acceptance_measure: false }) }, errorMessage: String }));
vi.mock('./vocabulary', async original => ({ ...await original<typeof import('./vocabulary')>(), vocabulary: mocks.vocabulary }));
vi.mock('@tauri-apps/api/window', () => ({ getCurrentWindow: () => ({ destroy: mocks.destroy, onCloseRequested: async (handler: typeof mocks.closeHandler) => { mocks.closeHandler = handler; return () => {}; } }) }));
let app: App;
let host: HTMLDivElement;
async function settle() { for (let i = 0; i < 20; i++) await Promise.resolve(); await nextTick(); }
async function settleTransition() { await new Promise(resolve => setTimeout(resolve, 80)); await settle(); }
function button(label: string) { return [...host.querySelectorAll('button')].find(item => item.textContent?.trim() === label)!; }
beforeEach(async () => {
  mocks.destroy.mockReset().mockResolvedValue(undefined);
  mocks.vocabulary.mockReset().mockImplementation(request => Promise.resolve(request.operation === 'list_words' ? { words: [], total: 0, next_cursor: null } : []));
  host = document.createElement('div'); document.body.append(host);
  app = createApp(LibraryView); app.mount(host); await settle();
});
afterEach(() => { app.unmount(); host.remove(); });

test('home exposes import beside the word list and opens data management', async () => {
  const data = vi.fn();
  app.unmount();
  app = createApp(LibraryView, { onData: data }); app.mount(host); await settle();
  expect(host.querySelector('.library-header')).not.toBeNull();
  expect(host.querySelector('.advanced-filters')).not.toBeNull();
  expect(button('导入数据')).toBeUndefined();
  expect(button('数据管理')).toBeTruthy();
  button('导入文件').click();
  expect(data).toHaveBeenCalledExactlyOnceWith([], 'import');
});

test('word selection opens a full-list drawer and its left handle closes it', async () => {
  const id = crypto.randomUUID();
  mocks.vocabulary.mockImplementation(request => Promise.resolve(request.operation === 'list_words'
    ? { words: [{ id, kanji: '猫', kana: 'ねこ', meaning: '猫', pos: [], sense_count: 1, revision: 1 }], total: 1, next_cursor: null }
    : request.operation === 'get_word'
      ? { content: { id, kanji: '猫', kana: 'ねこ', jlpt: null, pos: [], pitches: [], senses: [{ id: 'sense', meaning: '猫' }], examples: [], library_ids: [], note: '' }, revision: 1 }
      : []));
  app.unmount(); app = createApp(LibraryView); app.mount(host); await settle();
  expect(host.querySelector('.formal-detail')).toBeNull();
  host.querySelector<HTMLElement>('.formal-row')!.click(); await settle();
  expect(host.querySelector('.formal-detail')).not.toBeNull();
  expect(host.querySelector('.formal-workspace .formal-detail')).toBeNull();
  expect(host.querySelector('.formal-row')).not.toBeNull();
  host.querySelector<HTMLButtonElement>('[aria-label="收回词条详情"]')!.click(); await settle();
  expect(host.querySelector('.formal-detail.drawer-leave-active')).not.toBeNull();
  await settleTransition();
  expect(host.querySelector('.formal-detail')).toBeNull();
  expect(host.querySelector('.formal-row')).not.toBeNull();
});

test('detail delete previews impact, requires two confirmations, and closes after deletion', async () => {
  const id = crypto.randomUUID();
  let deleted = false;
  mocks.vocabulary.mockImplementation(request => {
    if (request.operation === 'list_words') return Promise.resolve({ words: deleted ? [] : [{ id, kanji: '猫', kana: 'ねこ', meaning: '猫', pos: [], sense_count: 1, revision: 3 }], total: deleted ? 0 : 1, next_cursor: null });
    if (request.operation === 'get_word') return Promise.resolve({ content: { id, kanji: '猫', kana: 'ねこ', jlpt: null, pos: [], pitches: [], senses: [{ id: 'sense', meaning: '猫' }], examples: [], library_ids: [], note: '' }, revision: 3 });
    if (request.operation === 'delete_impact') return Promise.resolve({ memberships: 1, cards: 2, events: 3 });
    if (request.operation === 'batch') { deleted = true; return Promise.resolve(1); }
    return Promise.resolve([]);
  });
  app.unmount(); app = createApp(LibraryView); app.mount(host); await settle();
  host.querySelector<HTMLElement>('.formal-row')!.click(); await settle();
  button('删除词条').click(); await settle();
  expect(mocks.vocabulary).toHaveBeenCalledWith({ operation: 'delete_impact', selected: [{ id, revision: 3 }] });
  expect(host.querySelector('[role="dialog"]')?.textContent).toContain('2 张卡片及 3 条历史事件');
  expect(mocks.vocabulary.mock.calls.some(([request]) => request.operation === 'batch')).toBe(false);
  button('我已了解，继续确认').click(); await settle();
  expect(mocks.vocabulary.mock.calls.some(([request]) => request.operation === 'batch')).toBe(false);
  button('确认执行').click(); await settle();
  expect(mocks.vocabulary).toHaveBeenCalledWith({ operation: 'batch', selected: [{ id, revision: 3 }], action: 'delete', target: null });
  await settleTransition();
  expect(host.querySelector('.formal-detail')).toBeNull();
  expect(host.textContent).toContain('已删除 1 个词条');
  expect(host.querySelector('.formal-row')).toBeNull();
});

test('closing a dirty library offers choices, continuing cancels exit intent', async () => {
  button('新建词库').click(); await settle();
  const input = host.querySelector<HTMLInputElement>('[role="dialog"] input')!;
  input.value = '草稿'; input.dispatchEvent(new Event('input', { bubbles: true })); await settle();
  const preventDefault = vi.fn();
  mocks.closeHandler!({ preventDefault }); await settle();
  expect(preventDefault).toHaveBeenCalledOnce();
  expect(button('保存并离开')).toBeTruthy();
  expect(host.querySelector('main')?.hasAttribute('inert')).toBe(true);
  button('继续编辑').click(); await settle();
  button('取消').click(); await settle();
  button('放弃修改').click(); await settle();
  expect(mocks.destroy).not.toHaveBeenCalled();
  expect(host.querySelector('.dialog-leave-active')).not.toBeNull();
  await settleTransition();
  expect(host.querySelector('[role="dialog"]')).toBeNull();
});

test('discarding dirty library after a window close completes exit', async () => {
  button('新建词库').click(); await settle();
  const input = host.querySelector<HTMLInputElement>('[role="dialog"] input')!;
  input.value = '草稿'; input.dispatchEvent(new Event('input', { bubbles: true })); await settle();
  mocks.closeHandler!({ preventDefault: vi.fn() }); await settle();
  button('放弃修改').click(); await settle();
  expect(mocks.destroy).toHaveBeenCalledOnce();
  expect(mocks.vocabulary.mock.calls.some(([request]) => request.operation === 'save_library')).toBe(false);
});

test('home no longer exposes removed tag operations', async () => {
  expect(host.textContent).not.toContain('管理标签');
  expect(host.textContent).not.toContain('添加标签');
  expect(mocks.vocabulary.mock.calls.some(([request]) => request.operation === 'tags')).toBe(false);
});
