import { afterEach, beforeEach, expect, test, vi } from 'vitest';
import { createApp, nextTick, type App } from 'vue';
import AppView from './views/DiagnosticsView.vue';

const mocks = vi.hoisted(() => ({
  search: vi.fn(),
  health: vi.fn().mockResolvedValue({ app_version: 'test', sqlite_version: 'test', platform: 'test' }),
  diagnostics: vi.fn().mockResolvedValue({ word_count: 2, journal_mode: 'wal' }),
  scheduler: vi.fn().mockResolvedValue([]),
  storage: vi.fn().mockResolvedValue({ state: 'ready', schema_version: 2, library_count: 0, word_count: 0, data_directory: '/test', error: null }),
}));
vi.mock('./api', () => ({ desktop: true, api: mocks, errorMessage: String }));
let app: App;
let host: HTMLDivElement;
const initial = [
  { id: 1, kanji: '働く', kana: 'はたらく', meaning: '工作' },
  { id: 2, kanji: '猫', kana: 'ねこ', meaning: '猫' },
];
async function settle() {
  for (let i = 0; i < 12; i++) await Promise.resolve();
  await nextTick();
}
function input(value: string) {
  const field = host.querySelector('input')!;
  field.value = value;
  field.dispatchEvent(new Event('input', { bubbles: true }));
  return field;
}
beforeEach(async () => {
  vi.useFakeTimers();
  mocks.search.mockReset().mockResolvedValue({ words: initial, next_cursor: null, elapsed_ms: 1 });
  host = document.createElement('div'); document.body.append(host);
  app = createApp(AppView); app.mount(host); await settle();
  mocks.search.mockClear();
});
afterEach(() => { app.unmount(); host.remove(); vi.useRealTimers(); });

test('composition defers searches and preserves typing keys', async () => {
  const field = host.querySelector('input')!;
  field.dispatchEvent(new CompositionEvent('compositionstart', { bubbles: true }));
  input('はたらく 123 jk');
  field.dispatchEvent(new KeyboardEvent('keydown', { key: 'j', bubbles: true, isComposing: true }));
  await vi.advanceTimersByTimeAsync(300);
  expect(mocks.search).not.toHaveBeenCalled();
  expect(host.querySelector('[aria-selected="true"]')?.textContent).toContain('働く');
  field.dispatchEvent(new CompositionEvent('compositionend', { bubbles: true }));
  await vi.advanceTimersByTimeAsync(200); await settle();
  expect(mocks.search).toHaveBeenCalledExactlyOnceWith('はたらく 123 jk', 0);
});

test('a stale response cannot replace the latest query', async () => {
  let resolveOld!: (value: unknown) => void;
  mocks.search.mockImplementationOnce(() => new Promise(resolve => { resolveOld = resolve; }));
  input('old'); await vi.advanceTimersByTimeAsync(200);
  input('new'); await vi.advanceTimersByTimeAsync(200); await settle();
  resolveOld({ words: [{ id: 99, kanji: 'STALE', kana: '', meaning: '' }], next_cursor: null, elapsed_ms: 1 });
  await settle();
  expect(host.textContent).not.toContain('STALE');
  expect(host.textContent).toContain('已加载 2 条');
});

test('verification failures reach the visible event boundary', async () => {
  mocks.diagnostics.mockRejectedValueOnce(new Error('diagnostics failed'));
  mocks.scheduler.mockClear();
  host.querySelector<HTMLButtonElement>('header button')!.click();
  await settle();
  expect(host.querySelector('[role="alert"]')?.textContent).toContain('diagnostics failed');
  expect(mocks.scheduler).not.toHaveBeenCalled();
  expect(host.querySelector('header button')!.hasAttribute('disabled')).toBe(false);
});

test('stale failures cannot overwrite a newer successful query', async () => {
  let rejectOld!: (cause: Error) => void;
  mocks.search.mockImplementationOnce(() => new Promise((_, reject) => { rejectOld = reject; }));
  input('old');
  await vi.advanceTimersByTimeAsync(200);
  input('new');
  await vi.advanceTimersByTimeAsync(200);
  await settle();
  rejectOld(new Error('stale failure'));
  await settle();
  expect(host.querySelector('[role="alert"]')).toBeNull();
  expect(host.textContent).toContain('已加载 2 条');
});

test('pagination appends results and keeps keyboard selection', async () => {
  mocks.search.mockResolvedValueOnce({ words: initial, next_cursor: 2, elapsed_ms: 1 });
  input('page');
  await vi.advanceTimersByTimeAsync(200);
  await settle();
  host.querySelector('[role="listbox"]')!.dispatchEvent(new KeyboardEvent('keydown', { key: 'j', bubbles: true }));
  await settle();
  expect(host.querySelector('[aria-selected="true"]')?.textContent).toContain('猫');
  mocks.search.mockResolvedValueOnce({ words: [{ id: 3, kanji: '学校', kana: 'がっこう', meaning: '学校' }], next_cursor: null, elapsed_ms: 1 });
  host.querySelector<HTMLButtonElement>('footer button')!.click();
  await settle();
  expect(mocks.search).toHaveBeenLastCalledWith('page', 2);
  expect(host.textContent).toContain('已加载 3 条');
  expect(host.querySelector('[aria-selected="true"]')?.textContent).toContain('猫');
  expect(host.querySelector('footer button')).toBeNull();
});

test('unmount cancels scheduled searches', async () => {
  input('pending');
  app.unmount();
  await vi.advanceTimersByTimeAsync(300);
  expect(mocks.search).not.toHaveBeenCalled();
  app = createApp(AppView);
  app.mount(host);
  await settle();
});

test('formal storage is distinct from synthetic data and blocked errors stay visible', async () => {
  expect(host.querySelector('[aria-label="正式数据底座"]')?.textContent).toContain('0 个真实词条');
  mocks.storage.mockResolvedValueOnce({ state: 'blocked', schema_version: null, data_directory: '/test', error: { code: 'SCHEMA_TOO_NEW', message: '请升级应用' } });
  host.querySelector('header button')!.dispatchEvent(new MouseEvent('click', { bubbles: true }));
  await settle();
  expect(host.querySelector('[aria-label="正式数据底座"]')?.textContent).toContain('存储受阻');
  expect(host.querySelector('[role="alert"]')?.textContent).toContain('SCHEMA_TOO_NEW');
  expect(host.querySelector('[aria-label="正式数据底座"]')?.textContent).not.toContain('0 个真实词条');
});
