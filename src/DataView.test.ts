import { afterEach, beforeEach, expect, test, vi } from 'vitest';
import { createApp, nextTick, type App } from 'vue';
import DataView from './views/DataView.vue';

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), close: undefined as undefined | ((event: { preventDefault: () => void }) => void) }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: mocks.invoke }));
vi.mock('./api', () => ({ desktop: true, errorMessage: (value: unknown) => (value as { message?: string }).message || String(value) }));
vi.mock('./vocabulary', () => ({ vocabulary: async () => [{ id: 'library', name: '词库' }] }));
vi.mock('@tauri-apps/api/window', () => ({ getCurrentWindow: () => ({ onDragDropEvent: async () => () => {}, onCloseRequested: async (handler: typeof mocks.close) => { mocks.close = handler; return () => {}; } }) }));
let app: App;
let host: HTMLDivElement;
async function settle() { for (let i = 0; i < 30; i++) await Promise.resolve(); await nextTick(); }
function button(label: string) { return [...host.querySelectorAll('button')].find(item => item.textContent?.trim() === label)!; }
beforeEach(async () => {
  mocks.invoke.mockReset();
  host = document.createElement('div'); document.body.append(host);
  app = createApp(DataView); app.mount(host); await settle();
});
afterEach(() => { app.unmount(); host.remove(); vi.useRealTimers(); });

test('preview errors require explicit valid-only confirmation and are shown before commit', async () => {
  mocks.invoke.mockImplementation(async (command, args) => {
    if (command === 'exchange_file') return '/source.json';
    if (command === 'exchange') return { job_id: args.request.operation };
    if (command === 'exchange_status') return { id: args.id, state: 'completed', processed: 2, cancellable: false, result: args.id === 'preview' ? { task_id: 'task', valid: 1, errors: 1, total: 2, format: 'json', columns: [] } : [{ line: 2, content: null, error: '假名缺失', candidates: [], matched: null }] };
  });
  button('选择文件').click(); await settle();
  button('全量校验并预览').click(); await settle();
  expect(host.textContent).toContain('假名缺失');
  expect(button('确认并原子提交').disabled).toBe(true);
  const checkbox = [...host.querySelectorAll<HTMLInputElement>('input[type=checkbox]')].find(input => input.parentElement?.textContent?.includes('我确认仅导入有效行'))!;
  checkbox.checked = true; checkbox.dispatchEvent(new Event('change', { bubbles: true })); await settle();
  expect(button('确认并原子提交').disabled).toBe(false);
  expect(mocks.invoke.mock.calls.some(([command, args]) => command === 'exchange' && args.request.operation === 'commit')).toBe(false);
});

test('cached data view keeps preview when navigating back to the library', async () => {
  const { KeepAlive, defineComponent, h, ref } = await import('vue');
  app.unmount();
  const visible = ref(true);
  app = createApp(defineComponent({ setup: () => () => h(KeepAlive, {}, { default: () => visible.value ? h(DataView, { onBack: () => { visible.value = false; } }) : null }) }));
  app.mount(host); await settle();
  const field = [...host.querySelectorAll<HTMLInputElement>('input')].find(input => input.parentElement?.textContent?.includes('文件（也可拖入）'))!;
  field.value = '/keep-me.csv'; field.dispatchEvent(new Event('input', { bubbles: true })); await settle();
  button('返回词库').click(); await settle();
  visible.value = true; await settle();
  expect([...host.querySelectorAll<HTMLInputElement>('input')].some(input => input.value === '/keep-me.csv')).toBe(true);
});

test('explicit selection exports ids without a library scope', async () => {
  app.unmount();
  app = createApp(DataView, { selected: ['word-one', 'word-two'] }); app.mount(host); await settle();
  mocks.invoke.mockImplementation(async (command, args) => {
    if (command === 'exchange_file') return '/export.json';
    if (command === 'exchange_target') return null;
    if (command === 'exchange') return { job_id: args.request.operation };
    if (command === 'exchange_status') return { state: 'completed', result: { words: 2 } };
  });
  button('导出 JSON').click(); await settle();
  expect(mocks.invoke).toHaveBeenCalledWith('exchange', { request: { operation: 'export', options: { path: '/export.json', format: 'json', library_id: null, selected: ['word-one', 'word-two'], csv_loss_confirmed: false, csv_original: false, expected: null } } });
});

test('running task blocks close and exposes cancellation independently', async () => {
  vi.useFakeTimers();
  mocks.invoke.mockImplementation(async (command) => {
    if (command === 'exchange_file') return '/source.json';
    if (command === 'exchange') return { job_id: 'running' };
    if (command === 'exchange_status') return { id: 'running', state: 'running', processed: 10, cancellable: true };
  });
  button('选择文件').click(); await settle();
  button('全量校验并预览').click(); await settle();
  const preventDefault = vi.fn(); mocks.close!({ preventDefault });
  expect(preventDefault).toHaveBeenCalledOnce();
  button('取消当前任务').click(); await settle();
  expect(mocks.invoke).toHaveBeenCalledWith('exchange_status', { id: 'running', cancel: true });
  mocks.invoke.mockResolvedValue({ id: 'running', state: 'cancelled', error: { message: '已取消' } });
  await vi.advanceTimersByTimeAsync(200); await settle();
  expect(host.textContent).toContain('已取消');
});
