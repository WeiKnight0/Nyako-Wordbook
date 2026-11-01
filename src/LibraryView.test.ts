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

test('home exposes import beside the word list and opens data management', async () => {
  const data = vi.fn();
  app.unmount();
  app = createApp(LibraryView, { onData: data }); app.mount(host); await settle();
  expect(host.querySelector('.library-header')).not.toBeNull();
  expect(host.querySelector('.advanced-filters')).not.toBeNull();
  button('导入文件').click();
  expect(data).toHaveBeenCalledOnce();
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
