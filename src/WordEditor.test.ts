import { afterEach, beforeEach, expect, test, vi } from 'vitest';
import { createApp, nextTick, type App } from 'vue';
import WordEditor from './components/WordEditor.vue';
import type { WordDetail } from './vocabulary';

const mocks = vi.hoisted(() => ({ vocabulary: vi.fn() }));
vi.mock('./vocabulary', async importOriginal => ({ ...await importOriginal<typeof import('./vocabulary')>(), vocabulary: mocks.vocabulary }));
let app: App;
let host: HTMLDivElement;
const word = (): WordDetail => ({ content: { id: crypto.randomUUID(), kanji: '猫', kana: 'ねこ', jlpt: null, note: '', senses: [{ id: crypto.randomUUID(), meaning: '猫' }], examples: [], pitches: [], pos: [], library_ids: [] }, revision: 1, created_at: 1, updated_at: 1, card_state: 'new', due_at: null, suspended: false, buried_until: null });
async function settle() { for (let i = 0; i < 10; i++) await Promise.resolve(); await nextTick(); }
function button(text: string) { return [...host.querySelectorAll('button')].find(button => button.textContent?.trim() === text)!; }
function editMeaning(value: string) {
  const field = host.querySelector<HTMLTextAreaElement>('[aria-label="义项 1"]')!;
  field.value = value; field.dispatchEvent(new Event('input', { bubbles: true }));
}
beforeEach(() => {
  mocks.vocabulary.mockReset();
  host = document.createElement('div'); document.body.append(host);
});
afterEach(() => { app.unmount(); host.remove(); vi.useRealTimers(); });

test('failed saves preserve draft and dirty close offers all choices', async () => {
  mocks.vocabulary.mockRejectedValue({ code: 'DISK_FULL', message: '磁盘已满' });
  app = createApp(WordEditor, { word: word(), libraries: [], libraryId: null }); app.mount(host);
  editMeaning('本地修改'); await settle();
  host.querySelector('form')!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }));
  await settle();
  expect(host.querySelector('[role="alert"]')?.textContent).toContain('磁盘已满');
  expect(host.querySelector<HTMLTextAreaElement>('[aria-label="义项 1"]')!.value).toBe('本地修改');
  button('取消').click(); await settle();
  expect(button('保存并离开')).toBeTruthy();
  expect(button('放弃修改')).toBeTruthy();
  expect(button('继续编辑')).toBeTruthy();
});

test('conflict compares latest content and reload is explicit', async () => {
  const initial = word();
  const latest = { ...initial, content: { ...initial.content, note: '另一个版本' }, revision: 2 };
  mocks.vocabulary.mockImplementation(request => request.operation === 'save_word'
    ? Promise.reject({ code: 'REVISION_CONFLICT', message: '版本冲突' }) : Promise.resolve(latest));
  app = createApp(WordEditor, { word: initial, libraries: [], libraryId: null }); app.mount(host);
  editMeaning('未覆盖草稿');
  host.querySelector('form')!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }));
  await settle();
  expect(host.textContent).toContain('另一个版本');
  expect(host.querySelector<HTMLTextAreaElement>('[aria-label="义项 1"]')!.value).toBe('未覆盖草稿');
  button('放弃本地修改并重载最新版本').click(); await settle();
  expect(host.querySelector<HTMLTextAreaElement>('[aria-label="义项 1"]')!.value).toBe('猫');
});

test('composition and repeat cannot invoke save shortcut', async () => {
  app = createApp(WordEditor, { word: word(), libraries: [], libraryId: null }); app.mount(host);
  const field = host.querySelector('input')!;
  field.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', ctrlKey: true, isComposing: true, bubbles: true }));
  field.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', ctrlKey: true, repeat: true, bubbles: true }));
  await settle(); expect(mocks.vocabulary).not.toHaveBeenCalled();
});

test('removing a sense retains its example as unassigned', async () => {
  const initial = word();
  initial.content.examples.push({ id: crypto.randomUUID(), sense_id: initial.content.senses[0]!.id, jp: '猫です', cn: null });
  initial.content.senses.push({ id: crypto.randomUUID(), meaning: '第二义项' });
  mocks.vocabulary.mockImplementation(request => Promise.resolve({ ...initial, content: request.content, revision: 2 }));
  app = createApp(WordEditor, { word: initial, libraries: [], libraryId: null }); app.mount(host);
  button('删除义项').click(); await settle();
  host.querySelector('form')!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }));
  await settle();
  expect(mocks.vocabulary.mock.calls[0]![0].content.examples[0].sense_id).toBeNull();
  expect(mocks.vocabulary.mock.calls[0]![0].content.examples[0].jp).toBe('猫です');
});

test('candidate lookup waits for composition confirmation and debounce', async () => {
  vi.useFakeTimers();
  mocks.vocabulary.mockResolvedValue({ words: [], next_cursor: null });
  app = createApp(WordEditor, { libraries: [], libraryId: null }); app.mount(host);
  const field = host.querySelectorAll('input')[1]!;
  field.dispatchEvent(new CompositionEvent('compositionstart', { bubbles: true }));
  field.value = 'ねこ';
  field.dispatchEvent(new Event('input', { bubbles: true }));
  await settle();
  await vi.advanceTimersByTimeAsync(300);
  expect(mocks.vocabulary).not.toHaveBeenCalled();
  field.dispatchEvent(new CompositionEvent('compositionend', { bubbles: true }));
  await settle();
  await vi.advanceTimersByTimeAsync(199);
  expect(mocks.vocabulary).not.toHaveBeenCalled();
  await vi.advanceTimersByTimeAsync(1);
  expect(mocks.vocabulary).toHaveBeenCalledTimes(1);
});

test('blank writing cannot be saved', async () => {
  app = createApp(WordEditor, { word: word(), libraries: [], libraryId: null }); app.mount(host);
  const field = host.querySelector<HTMLInputElement>('input[lang="ja"]')!;
  field.value = ''; field.dispatchEvent(new Event('input', { bubbles: true }));
  host.querySelector('form')!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }));
  await settle();
  expect(mocks.vocabulary).not.toHaveBeenCalled();
});

test('late candidate response is ignored after input changes and pending lookup is cancelled on unmount', async () => {
  vi.useFakeTimers();
  let resolve!: (page: unknown) => void;
  mocks.vocabulary.mockReturnValue(new Promise(done => { resolve = done; }));
  app = createApp(WordEditor, { libraries: [], libraryId: null }); app.mount(host);
  const field = host.querySelectorAll('input')[1]!;
  field.value = 'ねこ'; field.dispatchEvent(new Event('input', { bubbles: true })); await settle();
  await vi.advanceTimersByTimeAsync(200);
  field.value = 'いぬ'; field.dispatchEvent(new Event('input', { bubbles: true })); await settle();
  resolve({ words: [{ kana: 'ねこ', kanji: null, meaning: '过期候选' }], next_cursor: null });
  await settle();
  expect(host.textContent).not.toContain('过期候选');
  app.unmount();
  await vi.advanceTimersByTimeAsync(500);
  expect(mocks.vocabulary).toHaveBeenCalledTimes(1);
  app = createApp({ render: () => null }); app.mount(host);
});
