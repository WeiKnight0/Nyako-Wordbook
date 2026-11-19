import { afterEach, beforeEach, expect, test, vi } from 'vitest';
import { createApp, nextTick, type App } from 'vue';
import DataView from './views/DataView.vue';

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), libraries: vi.fn(), close: undefined as undefined | ((event: { preventDefault: () => void }) => void) }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: mocks.invoke }));
vi.mock('./api', () => ({ desktop: true, errorMessage: (value: unknown) => (value as { message?: string }).message || String(value) }));
vi.mock('./vocabulary', () => ({ vocabulary: mocks.libraries }));
vi.mock('@tauri-apps/api/window', () => ({ getCurrentWindow: () => ({ onDragDropEvent: async () => () => {}, onCloseRequested: async (handler: typeof mocks.close) => { mocks.close = handler; return () => {}; } }) }));
let app: App;
let host: HTMLDivElement;
async function settle() { for (let i = 0; i < 60; i++) await Promise.resolve(); await nextTick(); }
function fill(label: string, value: string) {
  const field = [...host.querySelectorAll<HTMLInputElement>('label input')].find(input => input.parentElement?.textContent?.includes(label))!;
  field.value = value; field.dispatchEvent(new Event('input', { bubbles: true }));
}
function previewResult(overrides: Record<string, unknown> = {}) { return { task_id: 'task', path: '/source.json', source_namespace: 'source', valid: 1, errors: 0, total: 1, format: 'json', columns: [], ...overrides }; }
function summaryResult(overrides: Record<string, unknown> = {}) { return { new: 1, same: 0, update: 0, ambiguous: 0, error: 0, unresolved_lines: [], source_namespace: 'source', ...overrides }; }
async function settleTransition() { await new Promise(resolve => setTimeout(resolve, 30)); await settle(); }
function button(label: string) { return [...host.querySelectorAll('button')].find(item => item.textContent?.trim() === label)!; }
beforeEach(async () => {
  mocks.invoke.mockReset();
  mocks.libraries.mockReset().mockResolvedValue([{ id: 'library', name: '词库' }]);
  host = document.createElement('div'); document.body.append(host);
  app = createApp(DataView); app.mount(host); await settle();
});
afterEach(() => { app.unmount(); host.remove(); vi.useRealTimers(); });

test('data tasks use a full-width workspace and keep import available from the first screen', async () => {
  const main = host.querySelector('main.data-view')!;
  expect(main.querySelector('.data-tabs [aria-current="page"]')?.textContent).toBe('导入');
  expect(main.querySelector('[aria-label="导入词条"]')).not.toBeNull();
  expect(button('选择文件')).not.toBeNull();
  button('备份与恢复').click(); await settle();
  expect(main.querySelector('.data-tabs [aria-current="page"]')?.textContent).toBe('备份与恢复');
  button('导入').click(); await settle();
  expect(main.querySelector('.data-tabs [aria-current="page"]')?.textContent).toBe('导入');
});

test('preview errors require explicit valid-only confirmation and are shown before commit', async () => {
  mocks.invoke.mockImplementation(async (command, args) => {
    if (command === 'exchange_file') return '/source.json';
    if (command === 'exchange') return { job_id: args.request.operation };
    if (command === 'exchange_status') return { id: args.id, state: 'completed', processed: 2, cancellable: false, result: args.id === 'preview' ? previewResult({ errors: 1, total: 2 }) : args.id === 'import_summary' ? summaryResult({ error: 1 }) : [{ line: 2, content: null, error: '假名缺失', candidates: [], matched: null, present: [] }] };
  });
  button('选择文件').click(); await settle();
  button('全量校验并预览').click(); await settle();
  expect(host.textContent).toContain('假名缺失');
  expect(button('确认并原子提交').disabled).toBe(true);
  fill('新建词库', '导入词库');
  const checkbox = [...host.querySelectorAll<HTMLInputElement>('input[type=checkbox]')].find(input => input.parentElement?.textContent?.includes('我确认仅导入有效行'))!;
  checkbox.checked = true; checkbox.dispatchEvent(new Event('change', { bubbles: true })); await settle();
  expect(button('确认并原子提交').disabled).toBe(false);
  button('确认并原子提交').click(); await settle();
  expect(host.querySelector('[role="dialog"]')?.textContent).toContain('跳过 1 条错误行');
  expect(mocks.invoke.mock.calls.some(([command, args]) => command === 'exchange' && args.request.operation === 'commit')).toBe(false);
  button('取消').click(); await settle();
  expect(host.querySelector('.dialog-leave-active')).not.toBeNull();
  await settleTransition();
  expect(host.querySelector('[role="dialog"]')).toBeNull();
  expect(host.textContent).toContain('假名缺失');
});

test('cached data view keeps preview when navigating back to the library', async () => {
  const { KeepAlive, defineComponent, h, ref } = await import('vue');
  app.unmount();
  const visible = ref(true);
  app = createApp(defineComponent({ setup: () => () => h(KeepAlive, {}, { default: () => visible.value ? h(DataView, { onBack: () => { visible.value = false; } }) : null }) }));
  app.mount(host); await settle();
  const field = [...host.querySelectorAll<HTMLInputElement>('input')].find(input => input.parentElement?.textContent?.includes('文件路径（也可拖入）'))!;
  field.value = '/keep-me.csv'; field.dispatchEvent(new Event('input', { bubbles: true })); await settle();
  button('← 返回词库').click(); await settle();
  visible.value = true; await settle();
  expect([...host.querySelectorAll<HTMLInputElement>('input')].some(input => input.value === '/keep-me.csv')).toBe(true);
});

test('single-sheet Excel previews ten columns without mapping', async () => {
  mocks.invoke.mockImplementation(async (command, args) => {
    if (command === 'exchange_file') return '/words.xlsx';
    if (command === 'exchange') return { job_id: args.request.operation };
    if (command === 'exchange_status') return { state: 'completed', result: args.id === 'inspect_excel' ? { sheets:['词条'], sheet:'词条', columns:['词条标识','写法','假名','释义','词性','音调','JLPT','日文例句','例句中文翻译','备注'], samples:[['cat','猫','ねこ','猫']] } : args.id === 'preview' ? previewResult({ path: '/words.xlsx', source_namespace: 'nyako:excel:local', format: 'xlsx' }) : args.id === 'import_summary' ? summaryResult() : [] };
  });
  button('选择文件').click(); await settle();
  button('查看样本').click(); await settle();
  expect(host.textContent).toContain('ねこ');
  button('全量校验并预览').click(); await settle();
  expect(mocks.invoke).toHaveBeenCalledWith('exchange', { request: { operation:'preview', path:'/words.xlsx', options:{ source_namespace:'nyako:excel:local',excel:{sheet:null,headers:true} } } });
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
  expect(mocks.invoke).toHaveBeenCalledWith('exchange', { request: { operation: 'export', options: { path: '/export.json', format: 'json', library_id: null, selected: ['word-one', 'word-two'], expected: null } } });
  expect(host.querySelector('[role="dialog"]')?.textContent).toContain('JSON 已导出');
});

test('existing export target requires in-app confirmation and cancel does not write', async () => {
  app.unmount(); app = createApp(DataView, { selected: ['word-one'] }); app.mount(host); await settle();
  mocks.invoke.mockImplementation(async command => command === 'exchange_file' ? '/existing.xlsx' : command === 'exchange_target' ? { size: 10 } : { job_id: 'export' });
  button('导出 Excel').click(); await settle();
  expect(host.querySelector('[role="dialog"]')?.textContent).toContain('/existing.xlsx');
  button('取消').click(); await settle();
  expect(mocks.invoke.mock.calls.some(([command]) => command === 'exchange')).toBe(false);
});

test('file target lookup failure appears in a result dialog', async () => {
  app.unmount(); app = createApp(DataView, { selected: ['word-one'] }); app.mount(host); await settle();
  mocks.invoke.mockImplementation(async command => {
    if (command === 'exchange_file') return '/export.xlsx';
    if (command === 'exchange_target') throw new Error('路径不可访问');
  });
  button('导出 Excel').click(); await settle();
  expect(host.querySelector('[role="dialog"]')?.textContent).toContain('路径不可访问');
  expect(mocks.invoke.mock.calls.some(([command]) => command === 'exchange')).toBe(false);
});

test('reentering cached data view chooses requested section without clearing import path', async () => {
  app.unmount();
  const { KeepAlive, defineComponent, h, ref } = await import('vue');
  const visible = ref(true);
  const entry = ref(1);
  const target = ref<'import' | 'export'>('import');
  app = createApp(defineComponent({ setup: () => () => h(KeepAlive, {}, { default: () => visible.value ? h(DataView, { entry: entry.value, entrySection: target.value }) : null }) }));
  app.mount(host); await settle();
  const path = host.querySelector<HTMLInputElement>('.file-picker input')!;
  path.value = '/words.xlsx'; path.dispatchEvent(new Event('input', { bubbles: true })); await settle();
  visible.value = false; await settle();
  target.value = 'export'; entry.value++; visible.value = true; await settle();
  expect(host.querySelector('.data-tabs [aria-current="page"]')?.textContent).toBe('导出');
  expect(host.querySelector<HTMLInputElement>('.file-picker input')?.value).toBe('/words.xlsx');
});

test('changing the source invalidates preview, decisions and valid-only consent', async () => {
  mocks.invoke.mockImplementation(async (command, args) => {
    if (command === 'exchange') return { job_id: args.request.operation };
    if (command === 'exchange_status') return { state: 'completed', result: args.id === 'preview' ? previewResult({ errors: 1, total: 2 }) : args.id === 'import_summary' ? summaryResult({ error: 1 }) : [] };
  });
  fill('文件路径', '/source.json'); await settle();
  button('全量校验并预览').click(); await settle();
  fill('新建词库', '目标');
  const consent = [...host.querySelectorAll<HTMLInputElement>('input[type=checkbox]')].find(input => input.parentElement?.textContent?.includes('我确认仅导入有效行'))!;
  consent.checked = true; consent.dispatchEvent(new Event('change', { bubbles: true })); await settle();
  expect(button('确认并原子提交').disabled).toBe(false);
  fill('来源标识', 'another-source'); await settle();
  expect(button('确认并原子提交')).toBeUndefined();
  expect(host.textContent).toContain('请重新校验后提交');
  button('全量校验并预览').click(); await settle();
  expect(button('确认并原子提交').disabled).toBe(true);
  expect([...host.querySelectorAll<HTMLInputElement>('input[type=checkbox]')].find(input => input.parentElement?.textContent?.includes('我确认仅导入有效行'))?.checked).toBe(false);
});

test('whole-file conflicts block commit and can be located beyond the first page', async () => {
  let pageOffset = 0;
  mocks.invoke.mockImplementation(async (command, args) => {
    if (command === 'exchange') { if (args.request.operation === 'page') pageOffset = args.request.offset; return { job_id: args.request.operation }; }
    if (command === 'exchange_status') return { state: 'completed', result: args.id === 'preview' ? previewResult({ valid: 51, total: 51 }) : args.id === 'import_summary' ? summaryResult({ new: 50, ambiguous: 1, unresolved_lines: [51] }) : pageOffset === 50 ? [{ line: 51, classification: 'ambiguous', present: ['kanji', 'kana', 'meaning'], content: { kanji: '猫', kana: 'ねこ', meaning: '猫' }, candidates: [{ id: 'existing', kana: 'ねこ', meaning: '猫', content: {} }], matched: null }] : [] };
  });
  fill('文件路径', '/source.json'); await settle();
  button('全量校验并预览').click(); await settle(); fill('新建词库', '目标'); await settle();
  expect(host.textContent).toContain('尚有 1 条身份需要确认');
  expect(button('确认并原子提交').disabled).toBe(true);
  button('定位待处理项').click(); await settle();
  expect(mocks.invoke).toHaveBeenCalledWith('exchange', { request: { operation: 'page', task_id: 'task', offset: 50, errors: false } });
  expect(document.activeElement?.id).toBe('import-row-51');
  const identity = host.querySelector<HTMLSelectElement>('#import-row-51 .choice-grid select')!;
  identity.value = 'skip'; identity.dispatchEvent(new Event('change', { bubbles: true })); await settle();
  expect(host.textContent).toContain('尚有 0 条身份需要确认');
  expect(button('确认并原子提交').disabled).toBe(false);
});

test('resuming a task restores file and actual source before committing and emits changes', async () => {
  const changed = vi.fn();
  app.unmount(); app = createApp(DataView, { onChanged: changed }); app.mount(host); await settle();
  mocks.invoke.mockImplementation(async (command, args) => {
    if (command === 'exchange') return { job_id: args.request.operation };
    if (command === 'exchange_status') return { state: 'completed', result: args.id === 'pending_imports' ? [previewResult()] : args.id === 'resume_import' ? previewResult({ path: '/original.xlsx', source_namespace: 'original-source' }) : args.id === 'import_summary' ? summaryResult() : args.id === 'commit' ? { created: 1, updated: 0, unchanged: 0, skipped: 0 } : [] };
  });
  button('检查未完成导入').click(); await settle();
  button('task · 1 条有效 · 重新核对').click(); await settle();
  expect(host.querySelector<HTMLInputElement>('.file-picker input')?.value).toBe('/original.xlsx');
  expect(host.querySelector<HTMLInputElement>('.import-settings input')?.value).toBe('original-source');
  fill('新建词库', '恢复目标'); await settle();
  button('确认并原子提交').click(); await settle();
  expect(host.querySelector('[role=dialog]')?.textContent).toContain('来源：original-source');
  expect(host.querySelector('[role=dialog]')?.textContent).toContain('恢复目标');
  expect(host.querySelector('.data-view > div')?.hasAttribute('inert')).toBe(true);
  button('确认执行').click(); await settle();
  expect(changed).toHaveBeenCalledOnce();
  await settleTransition();
  expect(host.querySelector('[role=dialog]')?.textContent).toContain('新增 1 条');
});

test('reactivating the data view refreshes library choices without dropping the import path', async () => {
  app.unmount();
  const { KeepAlive, defineComponent, h, ref } = await import('vue');
  const visible = ref(true);
  app = createApp(defineComponent({ setup: () => () => h(KeepAlive, {}, { default: () => visible.value ? h(DataView) : null }) }));
  app.mount(host); await settle(); fill('文件路径', '/kept.xlsx');
  visible.value = false; await settle();
  mocks.libraries.mockResolvedValue([{ id: 'new-library', name: '新建的词库' }]);
  visible.value = true; await settle();
  expect(host.querySelector<HTMLInputElement>('.file-picker input')?.value).toBe('/kept.xlsx');
  expect(host.querySelector('.destination select')?.textContent).toContain('新建的词库');
  expect(host.querySelector('.destination select')?.textContent).not.toContain('不指定词库');
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
