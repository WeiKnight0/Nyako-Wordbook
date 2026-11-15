import { afterEach, expect, test } from 'vitest';
import { createApp, h, nextTick, reactive, type App } from 'vue';
import WordList from './components/WordList.vue';
import type { WordRow } from './vocabulary';

let app: App;
let host: HTMLDivElement;
function row(index: number): WordRow {
  return {
    id: String(index), kanji: `词${index}`, kana: String(index), meaning: `释义${index}`,
    sense_count: 1, revision: 1, updated_at: 0, jlpt: null, pos: [], card_state: null,
    due_at: null, suspended: false, buried_until: null,
  };
}
function mount() {
  const props = reactive({ words: Array.from({ length: 100 }, (_, index) => row(index)), activeId: '80', selected: [], loading: false });
  host = document.createElement('div'); document.body.append(host);
  app = createApp({ render: () => h(WordList, props) }); app.mount(host);
  const viewport = host.querySelector<HTMLElement>('[role="listbox"]')!;
  Object.defineProperty(viewport, 'clientHeight', { value: 360 });
  return { props, viewport };
}
async function scroll(viewport: HTMLElement, top: number) {
  viewport.scrollTop = top;
  viewport.dispatchEvent(new Event('scroll'));
  await nextTick();
}
afterEach(() => { app.unmount(); host.remove(); });

test.each([false, true])('reordering results reveals the active option even when the first row stays unchanged: %s', async keepFirst => {
  const { props, viewport } = mount();
  await scroll(viewport, 80 * 72 + 18);
  viewport.focus();
  expect(viewport.getAttribute('aria-activedescendant')).toBe('formal-80');

  props.words = keepFirst ? [props.words[0]!, ...props.words.slice(1).reverse()] : [...props.words].reverse();
  await nextTick();
  expect(viewport.scrollTop).toBe(props.words.findIndex(word => word.id === '80') * 72);
  expect(host.querySelector('#formal-80')).not.toBeNull();
  expect(viewport.getAttribute('aria-activedescendant')).toBe('formal-80');
  expect(document.activeElement).toBe(viewport);
});

test('filtering and replacing the active word reveals its new neighbour', async () => {
  const { props, viewport } = mount();
  await scroll(viewport, 80 * 72);
  props.words = props.words.filter(word => Number(word.id) < 50);
  props.activeId = '10';
  await nextTick();
  expect(viewport.scrollTop).toBe(10 * 72);
  expect(viewport.getAttribute('aria-activedescendant')).toBe('formal-10');
  expect(host.querySelector('#formal-10')).not.toBeNull();
});

test.each(['refresh', 'append'] as const)('%s preserves scroll when existing result IDs keep their order', async change => {
  const { props, viewport } = mount();
  props.activeId = '0';
  await nextTick();
  await scroll(viewport, 80 * 72 + 18);
  props.words = change === 'refresh'
    ? props.words.map(word => ({ ...word, revision: 2, meaning: `新${word.meaning}` }))
    : [...props.words, ...Array.from({ length: 20 }, (_, index) => row(index + 100))];
  await nextTick();
  expect(viewport.scrollTop).toBe(80 * 72 + 18);
  expect(host.querySelector('#formal-80')).not.toBeNull();
  expect(host.querySelector('#formal-0')).toBeNull();
});
