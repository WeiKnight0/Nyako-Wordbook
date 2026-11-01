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
function button(label: string) { return [...host.querySelectorAll('button')].find(item => item.textContent?.trim() === label)!; }
beforeEach(async () => {
  mocks.destroy.mockReset().mockResolvedValue(undefined);
  mocks.vocabulary.mockReset().mockImplementation(request => Promise.resolve(request.operation === 'list_words' ? { words: [], total: 0, next_cursor: null } : []));
  host = document.createElement('div'); document.body.append(host);
  app = createApp(LibraryView); app.mount(host); await settle();
});
afterEach(() => { app.unmount(); host.remove(); });

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

test('tag write keeps its dialog open until the request settles', async () => {
  app.unmount();
  let resolve!: (value: unknown) => void;
  const pending = new Promise(done => { resolve = done; });
  mocks.vocabulary.mockImplementation(request => {
    if (request.operation === 'tags') return Promise.resolve([{ id: 'tag', name: '标签' }]);
    if (request.operation === 'save_tag') return pending;
    return Promise.resolve(request.operation === 'list_words' ? { words: [], total: 0, next_cursor: null } : []);
  });
  app = createApp(LibraryView); app.mount(host); await settle();
  button('管理').click(); await settle();
  host.querySelector('form')!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }));
  await settle();
  const dialog = host.querySelector<HTMLElement>('[role="dialog"]')!;
  dialog.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }));
  expect(host.querySelector('[role="dialog"]')).toBe(dialog);
  expect(button('关闭').disabled).toBe(true);
  expect(button('删除标签…').disabled).toBe(true);
  resolve([]); await settle();
  expect(host.querySelector('[role="dialog"]')).toBeNull();
});
