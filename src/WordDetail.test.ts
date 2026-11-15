import { afterEach, beforeEach, expect, test, vi } from 'vitest';
import { createApp, nextTick, reactive, h, type App } from 'vue';
import WordDetail from './components/WordDetail.vue';
import type { WordDetail as Entry } from './vocabulary';

let app: App;
let host: HTMLDivElement;
const entry = (): Entry => ({
  content: {
    id: 'word', kanji: '開く', kana: 'ひらく', jlpt: 'N4', note: '',
    senses: [{ id: 'open', meaning: '打开' }, { id: 'hold', meaning: '举办' }],
    examples: [], pitches: [], pos: ['godan_verb', 'transitive'], library_ids: [],
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

test('examples follow their stable sense IDs, preserving sense order, example order and whitespace', () => {
  const word = entry();
  word.content.senses[0]!.meaning = ' 打开\n  使张开 ';
  word.content.examples = [
    { id: 'hold-1', sense_id: 'hold', jp: '会を開く。', cn: '举办会议。' },
    { id: 'free-1', sense_id: null, jp: ' その他\n  の例 ', cn: null },
    { id: 'open-1', sense_id: 'open', jp: ' 窓を\n  開く。 ', cn: ' 打开\n  窗户。 ' },
    { id: 'open-2', sense_id: 'open', jp: '本を開く。', cn: null },
    { id: 'free-2', sense_id: null, jp: '別の例。', cn: '' },
  ];
  mount({ word });
  const senses = [...host.querySelectorAll('.detail-sense')];
  expect(senses.map(sense => sense.querySelector('.sense-meaning')!.textContent)).toEqual([' 打开\n  使张开 ', '举办']);
  expect([...senses[0]!.querySelectorAll('.example-jp')].map(node => node.textContent)).toEqual([' 窓を\n  開く。 ', '本を開く。']);
  expect(senses[0]!.querySelector('.example-cn')!.textContent).toBe(' 打开\n  窗户。 ');
  expect(senses[0]!.querySelectorAll('.example-cn')).toHaveLength(1);
  expect(senses[1]!.querySelector('.example-jp')!.textContent).toBe('会を開く。');
  expect([...host.querySelectorAll('.detail-unassigned .example-jp')].map(node => node.textContent)).toEqual([' その他\n  の例 ', '別の例。']);
  expect(host.querySelectorAll('.detail-example')).toHaveLength(5);
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
  Object.assign(word.content, { kanji, kana: 'ありがとう', jlpt: null, pos: [], note: '', examples: [], pitches: [] });
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

test('pitch names use the stored small-kana mora rules without inventing a contour', () => {
  const word = entry();
  Object.assign(word.content, { kana: 'きゃ っこう', note: ' 第一行\n  第二行 ', library_ids: ['library'] });
  word.content.pitches = [0, 1, 2, 4].map(downstep => ({ id: `pitch-${downstep}`, downstep }));
  mount({ word, libraries: [{ id: 'library', name: '日常\n  用语', description: '', archived: false, revision: 1, word_count: 1 }] });
  expect([...host.querySelectorAll('.detail-accent')].map(node => node.textContent)).toEqual(['0 平板型', '1 头高型', '2 中高型', '4 尾高型']);
  expect(host.querySelector('.detail-pitch')!.textContent).toContain('0 表示词内无下降；其他数字表示第几拍后音高下降');
  expect(host.querySelector('.plain-note')!.textContent).toBe(' 第一行\n  第二行 ');
  expect(host.querySelector('.detail-memberships p')!.textContent).toBe('日常\n  用语');
  expect(host.querySelector('.detail-meta')!.textContent).toContain('N4');
  expect(host.querySelector('.detail-meta')!.textContent).toContain('五段动词 / 他动词');
});
