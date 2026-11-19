import { afterEach, beforeEach, expect, test, vi } from 'vitest';
import { createApp, nextTick, reactive, h, type App } from 'vue';
import WordDetail from './components/WordDetail.vue';
import type { WordDetail as Entry } from './vocabulary';

let app: App;
let host: HTMLDivElement;
const entry = (): Entry => ({
  content: {
    id: 'word', kanji: '開く', kana: 'ひらく', jlpt: 'N4', note: '',
    meaning: '打开\n\n举办', examples_jp: '', examples_cn: '', pitch: '', pos: ['godan_verb', 'transitive'], library_ids: [],
  },
  revision: 1, created_at: 1, updated_at: 1, card_state: null, due_at: null, suspended: false, buried_until: null,
});
function mount(overrides: Partial<InstanceType<typeof WordDetail>['$props']> = {}) {
  const listeners = { onNavigate: vi.fn(), onClose: vi.fn(), onRetry: vi.fn(), onEdit: vi.fn(), onDelete: vi.fn() };
  const props = reactive({ word: entry(), libraries: [], hasPrevious: true, hasNext: true, ...overrides });
  app = createApp({ render: () => h(WordDetail, { ...props, ...listeners }) });
  app.mount(host);
  return { props, ...listeners };
}
function key(target: Element, name: string, options: KeyboardEventInit = {}) {
  const event = new KeyboardEvent('keydown', { key: name, bubbles: true, cancelable: true, ...options });
  target.dispatchEvent(event);
  return event;
}
const button = (label: string) => host.querySelector<HTMLButtonElement>(`button[aria-label="${label}"]`)!;
beforeEach(() => { host = document.createElement('div'); document.body.append(host); });
afterEach(() => { app.unmount(); host.remove(); });

test('free text preserves whitespace, numbering and independent translations', () => {
  const word = entry();
  Object.assign(word.content, { meaning: ' 打开\n\n3. 举办 ', examples_jp: ' 窓を\n  開く。\n本を開く。 ', examples_cn: ' 只有一句中文。 ' });
  mount({ word });
  expect(host.querySelector('[aria-label="释义"] .plain-text')!.textContent).toBe(word.content.meaning);
  expect(host.querySelector('[aria-label="日文例句"] .plain-text')!.textContent).toBe(word.content.examples_jp);
  expect(host.querySelector('[aria-label="例句中文翻译"] .plain-text')!.textContent).toBe(word.content.examples_cn);
  expect(host.querySelector('.detail-delete')).toBeNull();
});

test('toolbar exposes full result position and respects both boundaries and pending navigation', async () => {
  const { props, onNavigate, onClose } = mount({ position: 30, total: 120, hasPrevious: false });
  const next = button('下一个词条'); next.focus();
  expect(host.querySelector('aside.formal-detail[aria-label="词条详情"][tabindex="-1"]')).not.toBeNull();
  expect(host.querySelector('.drawer-handle')).toBeNull();
  expect(host.querySelector('.detail-position')!.textContent).toBe('第 30 / 120 条');
  expect(button('上一个词条').getAttribute('aria-disabled')).toBe('true');
  button('上一个词条').click();
  button('下一个词条').click();
  expect(onNavigate.mock.calls).toEqual([[1]]);
  props.navigating = true;
  await nextTick();
  expect(button('下一个词条').getAttribute('aria-disabled')).toBe('true');
  expect(next.disabled).toBe(false);
  expect(document.activeElement).toBe(next);
  expect(host.querySelector('.detail-content')!.getAttribute('aria-busy')).toBe('true');
  button('下一个词条').click();
  expect(key(host.querySelector('h2')!, 'ArrowRight').defaultPrevented).toBe(false);
  expect(onNavigate).toHaveBeenCalledTimes(1);
  props.navigating = false; props.hasPrevious = true; props.hasNext = false;
  await nextTick();
  expect(button('下一个词条').getAttribute('aria-disabled')).toBe('true');
  expect(next.disabled).toBe(false);
  expect(document.activeElement).toBe(next);
  button('上一个词条').click();
  expect(onNavigate.mock.calls).toEqual([[1], [-1]]);
  button('收回词条详情').click();
  expect(onClose).toHaveBeenCalledTimes(1);
  expect(button('收回词条详情').textContent).toContain('返回列表');
});

test('reading arrows navigate once while composition, repeats and modifiers keep native behavior', () => {
  const { onNavigate, onClose } = mount();
  const heading = host.querySelector('h2')!;
  expect(key(heading, 'ArrowLeft').defaultPrevented).toBe(true);
  expect(key(heading, 'ArrowRight').defaultPrevented).toBe(true);
  expect(onNavigate.mock.calls).toEqual([[-1], [1]]);
  [
    { isComposing: true }, { repeat: true }, { ctrlKey: true }, { metaKey: true },
    { altKey: true }, { shiftKey: true }, { keyCode: 229 },
  ].forEach(options => {
    expect(key(heading, 'ArrowRight', options).defaultPrevented).toBe(false);
    expect(key(heading, 'Escape', options).defaultPrevented).toBe(false);
  });
  expect(onNavigate).toHaveBeenCalledTimes(2);
  expect(onClose).not.toHaveBeenCalled();
  expect(key(heading, 'Escape').defaultPrevented).toBe(true);
  expect(onClose).toHaveBeenCalledTimes(1);
});

test.each([
  '<input>', '<textarea></textarea>', '<select><option>选项</option></select>',
  '<button><span>按钮</span></button>', '<a href="#"><span>链接</span></a>',
  '<details open><summary><span>更多操作</span></summary><p>详情</p></details>',
  '<div contenteditable="true"><span>编辑区</span></div>',
  '<div role="button"><span>操作</span></div>', '<div role="textbox">输入区</div>', '<div tabindex="0">控件</div>',
])('interactive target retains arrow behavior: %s', markup => {
  const { onNavigate } = mount();
  const container = host.querySelector('.detail-content')!;
  container.insertAdjacentHTML('beforeend', markup);
  const target = container.lastElementChild!.querySelector('span') ?? container.lastElementChild!;
  expect(key(target, 'ArrowLeft').defaultPrevented).toBe(false);
  expect(key(target, 'ArrowRight').defaultPrevented).toBe(false);
  expect(onNavigate).not.toHaveBeenCalled();
});

test('an unavailable direction is neither emitted nor intercepted', () => {
  const { onNavigate } = mount({ hasPrevious: false, hasNext: false });
  expect(key(host.querySelector('aside')!, 'ArrowLeft').defaultPrevented).toBe(false);
  expect(key(host.querySelector('aside')!, 'ArrowRight').defaultPrevented).toBe(false);
  expect(onNavigate).not.toHaveBeenCalled();
});

test('failed detail can retry and transitions through loading into a complete entry', async () => {
  const { props, onRetry, onClose } = mount({ word: undefined, error: '词条加载失败，请重试。' });
  expect(host.querySelector('[role="alert"]')!.textContent).toContain('词条加载失败');
  expect(host.querySelector('.detail-content')!.getAttribute('aria-busy')).toBe('false');
  host.querySelector<HTMLButtonElement>('.detail-error button')!.click();
  expect(onRetry).toHaveBeenCalledTimes(1);
  props.error = ''; props.navigating = true;
  await nextTick();
  expect(host.querySelector('[role="alert"]')).toBeNull();
  expect(host.querySelector('[role="status"]')!.textContent).toContain('正在加载词条');
  expect(host.querySelector('.detail-content')!.getAttribute('aria-busy')).toBe('true');
  props.word = entry(); props.navigating = false;
  await nextTick();
  expect(host.querySelector('h2')!.textContent).toBe('開く');
  expect(host.querySelector('[role="status"]')).toBeNull();
  key(button('收回词条详情'), 'Escape');
  expect(onClose).toHaveBeenCalledTimes(1);
});

test.each(['', 'ありがとう'])('kana-only entries avoid duplicate readings and omit empty optional fields (writing: %s)', kanji => {
  const word = entry();
  Object.assign(word.content, { kanji, kana: 'ありがとう', jlpt: null, pos: [], note: '', examples_jp: '', examples_cn: '', pitch: '' });
  mount({ word, readonly: true });
  expect(host.querySelector('h2')!.textContent).toBe('ありがとう');
  expect(host.querySelector('.detail-reading')).toBeNull();
  expect(host.querySelector('.detail-meta')).toBeNull();
  expect(host.querySelector('.detail-pitch')).toBeNull();
  expect(host.querySelector('.detail-unassigned')).toBeNull();
  expect(host.querySelector('.detail-note')).toBeNull();
  expect(host.querySelector('.detail-memberships')!.textContent).toContain('未分类');
  expect(host.querySelector('.detail-edit')).toBeNull();
  expect(host.querySelector('.detail-delete')).toBeNull();
  expect(host.textContent).not.toMatch(/未知|undefined|null/);
});

test('moving to another entry resets reading scroll without taking focus from navigation', async () => {
  const { props } = mount();
  const content = host.querySelector<HTMLElement>('.detail-content')!;
  const next = button('下一个词条'); next.focus(); content.scrollTop = 300;
  props.word = { ...entry(), content: { ...entry().content, id: 'another' } };
  await nextTick();
  expect(content.scrollTop).toBe(0);
  expect(document.activeElement).toBe(next);
});

test('pitch and notes display their original text', () => {
  const word = entry();
  Object.assign(word.content, { kana: 'きゃ っこう', note: ' 第一行\n  第二行 ', library_ids: ['library'] });
  word.content.pitch = '0 / 2、平板型（自记）';
  mount({ word, libraries: [{ id: 'library', name: '日常\n  用语', description: '', archived: false, revision: 1, word_count: 1 }] });
  expect(host.querySelector('.detail-pitch .plain-text')!.textContent).toBe(word.content.pitch);
  expect(host.querySelector('.plain-note')!.textContent).toBe(' 第一行\n  第二行 ');
  expect(host.querySelector('.detail-memberships p')!.textContent).toBe('日常\n  用语');
  expect(host.querySelector('.detail-meta')!.textContent).toContain('N4');
  expect(host.querySelector('.detail-meta')!.textContent).toContain('五段动词 / 他动词');
});
