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

test('reading opens beside the index and returning to the list preserves its rows', async () => {
  const id = crypto.randomUUID();
  mocks.vocabulary.mockImplementation(request => Promise.resolve(request.operation === 'list_words'
    ? { words: [{ id, kanji: '猫', kana: 'ねこ', meaning: '猫', pos: [], revision: 1 }], total: 1, next_cursor: null }
    : request.operation === 'get_word'
      ? { content: { id, kanji: '猫', kana: 'ねこ', jlpt: null, pos: [], pitch: '', meaning: '猫', examples_jp: '', examples_cn: '', library_ids: [], note: '' }, revision: 1 }
      : []));
  app.unmount(); app = createApp(LibraryView); app.mount(host); await settle();
  expect(host.querySelector('.reading-workspace .formal-detail h2')?.textContent).toBe('猫');
  button('列表').click(); await settle();
  expect(host.querySelector('.formal-detail')).toBeNull();
  host.querySelector<HTMLElement>('.formal-row')!.click(); await settle();
  expect(host.querySelector('.formal-workspace .formal-detail')).not.toBeNull();
  expect(host.querySelector('.formal-row')).not.toBeNull();
  host.querySelector<HTMLButtonElement>('[aria-label="收回词条详情"]')!.click(); await settle();
  expect(host.querySelector('.formal-detail')).toBeNull();
  expect(host.querySelector('.formal-row')).not.toBeNull();
});

test('list shows readable verb labels and keeps advanced filters collapsed', async () => {
  const id = crypto.randomUUID();
  mocks.vocabulary.mockImplementation(request => Promise.resolve(request.operation === 'list_words'
    ? { words: [{ id, kanji: '開く', kana: 'あく', meaning: '开', pos: ['godan_verb', 'intransitive'], revision: 1, jlpt: 'N4', card_state: 'new' }], total: 1, next_cursor: null }
    : request.operation === 'get_word'
      ? { content: { id, kanji: '開く', kana: 'あく', jlpt: 'N4', pos: ['godan_verb', 'intransitive'], pitch: '', meaning: '开', examples_jp: '', examples_cn: '', library_ids: [], note: '' }, revision: 1 }
      : []));
  app.unmount(); app = createApp(LibraryView); app.mount(host); await settle();
  expect(host.querySelector('.formal-row .row-meta')?.textContent).toContain('五段动词 / 自动词');
  expect(host.querySelector('.formal-row .row-meta')?.textContent).not.toContain('无卡片');
  expect(host.querySelector('.advanced-filters')?.hasAttribute('open')).toBe(false);
  host.querySelector<HTMLElement>('.formal-row')!.click(); await settle();
  expect(host.querySelector('.detail-meta')?.textContent).toContain('五段动词 / 自动词');
});

test('editor delete previews impact and closes reader after confirmation', async () => {
  const id = crypto.randomUUID();
  let deleted = false;
  mocks.vocabulary.mockImplementation(request => {
    if (request.operation === 'list_words') return Promise.resolve({ words: deleted ? [] : [{ id, kanji: '猫', kana: 'ねこ', meaning: '猫', pos: [], revision: 3 }], total: deleted ? 0 : 1, next_cursor: null });
    if (request.operation === 'get_word') return Promise.resolve({ content: { id, kanji: '猫', kana: 'ねこ', jlpt: null, pos: [], pitch: '', meaning: '猫', examples_jp: '', examples_cn: '', library_ids: [], note: '' }, revision: 3 });
    if (request.operation === 'delete_impact') return Promise.resolve({ memberships: 1, cards: 2, events: 3 });
    if (request.operation === 'batch') { deleted = true; return Promise.resolve(1); }
    return Promise.resolve([]);
  });
  app.unmount(); app = createApp(LibraryView); app.mount(host); await settle();
  host.querySelector<HTMLElement>('.formal-row')!.click(); await settle();
  button('编辑词条').click(); await settle();
  button('删除词条').click(); await settle();
  expect(mocks.vocabulary).toHaveBeenCalledWith({ operation: 'delete_impact', selected: [{ id, revision: 3 }] });
  expect(host.querySelector('[role="dialog"]')?.textContent).toContain('2 张卡片和 3 条学习记录');
  expect(mocks.vocabulary.mock.calls.some(([request]) => request.operation === 'batch')).toBe(false);
  button('确认删除').click(); await settle();
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

test('arrows and J/K move activity, Enter opens once, Escape restores focus and typing remains isolated', async () => {
  mocks.vocabulary.mockImplementation(request => Promise.resolve(request.operation === 'list_words'
    ? { words: ['a', 'b', 'c'].map(id => ({ id, kanji: id, kana: id, meaning: id, pos: [], revision: 1 })), total: 3, next_cursor: null }
    : request.operation === 'get_word'
      ? { content: { id: request.id, kanji: request.id, kana: request.id, jlpt: null, pos: [], pitch: '', meaning: '释义', examples_jp: '', examples_cn: '', library_ids: [], note: '' }, revision: 1 }
      : []));
  app.unmount(); app = createApp(LibraryView); app.mount(host); await settle();
  button('列表').click(); await settle(); mocks.vocabulary.mockClear();
  const list = host.querySelector<HTMLElement>('[role="listbox"]')!;
  list.focus();
  const key = async (value: string, options: KeyboardEventInit = {}) => { list.dispatchEvent(new KeyboardEvent('keydown', { key: value, bubbles: true, cancelable: true, ...options })); await settle(); };
  await key('ArrowDown'); await key('j'); await key('k'); await key('ArrowUp');
  expect(list.getAttribute('aria-activedescendant')).toBe('formal-a');
  expect(document.activeElement).toBe(list);
  expect(host.querySelector('[aria-label="词条详情"]')).toBeNull();
  expect(mocks.vocabulary.mock.calls.filter(([request]) => request.operation === 'get_word')).toHaveLength(0);
  await key('j', { isComposing: true }); await key('j', { repeat: true }); await key('j', { ctrlKey: true });
  expect(list.getAttribute('aria-activedescendant')).toBe('formal-a');
  await key('ArrowDown', { shiftKey: true });
  expect(host.querySelector<HTMLInputElement>('[aria-label="选择 a"]')!.checked).toBe(true);
  expect(host.querySelector<HTMLInputElement>('[aria-label="选择 b"]')!.checked).toBe(true);
  await key('Enter'); await key('Enter'); await key('Enter', { repeat: true });
  await settleTransition();
  const drawer = host.querySelector<HTMLElement>('[aria-label="词条详情"]')!;
  expect(drawer.querySelector('h2')?.textContent).toBe('b');
  expect(document.activeElement).toBe(drawer);
  expect(mocks.vocabulary.mock.calls.filter(([request]) => request.operation === 'get_word')).toHaveLength(1);
  drawer.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true })); await settleTransition();
  expect(host.querySelector('[aria-label="词条详情"]')).toBeNull();
  expect(document.activeElement).toBe(list);
  const input = host.querySelector<HTMLInputElement>('[aria-label="搜索真实词条"]')!;
  input.focus(); input.dispatchEvent(new KeyboardEvent('keydown', { key: 'j', bubbles: true })); await settle();
  expect(list.getAttribute('aria-activedescendant')).toBe('formal-b');
  host.querySelector<HTMLElement>('#formal-c')!.click(); await settleTransition();
  expect(host.querySelector('[aria-label="词条详情"] h2')?.textContent).toBe('c');
  expect(document.activeElement).toBe(host.querySelector('[aria-label="词条详情"]'));
});

test('closing during a detail load prevents a late request from reopening or stealing focus', async () => {
  let resolveOld!: (value: unknown) => void;
  const pending = new Promise(resolve => { resolveOld = resolve; });
  mocks.vocabulary.mockImplementation(request => {
    if (request.operation === 'list_words') return Promise.resolve({ words: ['a', 'b'].map(id => ({ id, kanji: id, kana: id, meaning: id, pos: [], revision: 1 })), total: 2, next_cursor: null });
    if (request.operation === 'get_word' && request.id === 'a') return pending;
    if (request.operation === 'get_word') return Promise.resolve({ content: { id: 'b', kanji: 'b', kana: 'b', jlpt: null, pos: [], pitch: '', meaning: '释义', examples_jp: '', examples_cn: '', library_ids: [], note: '' }, revision: 2 });
    return Promise.resolve([]);
  });
  app.unmount(); app = createApp(LibraryView); app.mount(host); await settle();
  host.querySelector<HTMLElement>('#formal-a')!.click(); await settleTransition();
  const drawer = host.querySelector<HTMLElement>('[aria-label="词条详情"]')!;
  expect(drawer.textContent).toContain('正在加载词条');
  expect(document.activeElement).toBe(drawer);
  drawer.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true })); await settleTransition();
  const list = host.querySelector<HTMLElement>('[role="listbox"]')!;
  list.dispatchEvent(new KeyboardEvent('keydown', { key: 'j', bubbles: true })); await settle();
  list.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true })); await settle();
  const current = host.querySelector<HTMLElement>('[aria-label="词条详情"]')!;
  expect(current.querySelector('h2')?.textContent).toBe('b');
  const search = host.querySelector<HTMLInputElement>('[aria-label="搜索真实词条"]')!;
  search.focus(); await settleTransition();
  expect(document.activeElement).toBe(search);
  resolveOld({ content: { id: 'a', kanji: 'a' }, revision: 1 }); await settle();
  expect(current.querySelector('h2')?.textContent).toBe('b');
  expect(document.activeElement).toBe(search);
  expect(mocks.vocabulary.mock.calls.filter(([request]) => request.operation === 'get_word')).toHaveLength(2);
});

test('library name input explains and enforces the backend limit', async () => {
  button('新建词库').click(); await settle();
  const field = host.querySelector<HTMLInputElement>('[role="dialog"] input')!;
  expect(field.maxLength).toBe(100);
  expect(host.querySelector(`#${field.getAttribute('aria-describedby')}`)?.textContent).toBe('最多 100 个字符');
});

test('reading follows keyboard selection and continues across page boundaries', async () => {
  const rows = ['a', 'b', 'c'].map(id => ({ id, kanji: id, kana: id, meaning: id, pos: [], revision: 1 }));
  mocks.vocabulary.mockImplementation(request => {
    if (request.operation === 'list_words') return Promise.resolve({ words: request.cursor ? [rows[2]] : rows.slice(0, 2), total: 3, next_cursor: request.cursor ? null : 'next' });
    if (request.operation === 'get_word') return Promise.resolve({ content: { id: request.id, kanji: request.id, kana: request.id, jlpt: null, pos: [], pitch: '', meaning: request.id, examples_jp: '', examples_cn: '', library_ids: [], note: '' }, revision: 1 });
    return Promise.resolve([]);
  });
  app.unmount(); app = createApp(LibraryView); app.mount(host); await settle();
  expect(host.querySelector('.formal-detail h2')?.textContent).toBe('a');
  const list = host.querySelector<HTMLElement>('[role="listbox"]')!; list.focus();
  list.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true })); await settle();
  expect(host.querySelector('.formal-detail h2')?.textContent).toBe('b');
  expect(document.activeElement).toBe(list);
  host.querySelector<HTMLButtonElement>('[aria-label="下一个词条"]')!.click(); await settle();
  expect(mocks.vocabulary).toHaveBeenCalledWith(expect.objectContaining({ operation: 'list_words', cursor: 'next' }));
  expect(host.querySelector('.formal-detail h2')?.textContent).toBe('c');
  expect(host.querySelector('.detail-position')?.textContent).toContain('3 / 3');
  expect(host.querySelector<HTMLButtonElement>('[aria-label="下一个词条"]')!.getAttribute('aria-disabled')).toBe('true');
  host.querySelector<HTMLButtonElement>('[aria-label="上一个词条"]')!.click(); await settle();
  expect(host.querySelector('.formal-detail h2')?.textContent).toBe('b');
});

test('failed reading keeps retry available and does not display the previous word', async () => {
  let failed = true;
  mocks.vocabulary.mockImplementation(request => {
    if (request.operation === 'list_words') return Promise.resolve({ words: [{ id: 'a', kanji: '猫', kana: 'ねこ', meaning: '猫', pos: [], revision: 1 }], total: 1, next_cursor: null });
    if (request.operation === 'get_word' && failed) return Promise.reject(new Error('读取失败'));
    if (request.operation === 'get_word') return Promise.resolve({ content: { id: 'a', kanji: '猫', kana: 'ねこ', jlpt: null, pos: [], pitch: '', meaning: '猫', examples_jp: '', examples_cn: '', library_ids: [], note: '' }, revision: 1 });
    return Promise.resolve([]);
  });
  app.unmount(); app = createApp(LibraryView); app.mount(host); await settle();
  expect(host.querySelector('.formal-detail [role="alert"]')?.textContent).toContain('读取失败');
  expect(host.querySelector('.formal-detail h2')).toBeNull();
  failed = false; button('重试').click(); await settle();
  expect(host.querySelector('.formal-detail h2')?.textContent).toBe('猫');
});

test('narrow windows open a dedicated reader and return to the same index', async () => {
  const change = { listener: undefined as undefined | ((event: { matches: boolean }) => void) };
  vi.stubGlobal('matchMedia', () => ({ matches: true, addEventListener: (_: string, listener: typeof change.listener) => { change.listener = listener; }, removeEventListener: vi.fn() }));
  mocks.vocabulary.mockImplementation(request => Promise.resolve(request.operation === 'list_words'
    ? { words: [{ id: 'a', kanji: '猫', kana: 'ねこ', meaning: '猫', pos: [], revision: 1 }], total: 1, next_cursor: null }
    : request.operation === 'get_word' ? { content: { id: 'a', kanji: '猫', kana: 'ねこ', jlpt: null, pos: [], pitch: '', meaning: '释义', examples_jp: '', examples_cn: '', library_ids: [], note: '' }, revision: 1 } : []));
  try {
    app.unmount(); app = createApp(LibraryView); app.mount(host); await settle();
    expect(host.querySelector('.formal-detail')).toBeNull();
    const list = host.querySelector<HTMLElement>('[role="listbox"]')!;
    host.querySelector<HTMLElement>('.formal-row')!.click(); await settle();
    expect(host.querySelector('.reader-page .formal-detail')).not.toBeNull();
    host.querySelector<HTMLButtonElement>('[aria-label="收回词条详情"]')!.click(); await settle();
    expect(host.querySelector('.formal-detail')).toBeNull();
    expect(host.querySelector('[role="listbox"]')).toBe(list);
    button('阅读').click(); await settle();
    expect(host.querySelector('.reader-page .formal-detail')).not.toBeNull();
    expect(document.activeElement).toBe(host.querySelector('.formal-detail'));
    change.listener!({ matches: false }); await settle();
    expect(host.querySelector('.reading-workspace .formal-detail')).not.toBeNull();
    change.listener!({ matches: true }); await settle();
    expect(host.querySelector('.formal-detail')).toBeNull();
    expect(document.activeElement).toBe(list);
  } finally { vi.unstubAllGlobals(); }
});

test('home no longer exposes removed tag operations', async () => {
  expect(host.textContent).not.toContain('管理标签');
  expect(host.textContent).not.toContain('添加标签');
  expect(mocks.vocabulary.mock.calls.some(([request]) => request.operation === 'tags')).toBe(false);
});
