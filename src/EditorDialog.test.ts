import { afterEach, expect, test, vi } from 'vitest';
import { createApp, h, nextTick, ref, type App } from 'vue';
import EditorDialog from './components/EditorDialog.vue';
import WordList from './components/WordList.vue';
import type { WordRow } from './vocabulary';

let app: App;
let host: HTMLDivElement;
afterEach(() => { app.unmount(); host.remove(); });
function mount(component: Parameters<typeof createApp>[0]) {
  host = document.createElement('div'); document.body.append(host);
  app = createApp(component); app.mount(host);
}

test('dialog traps focus excluding hidden ancestors, blocks busy Escape and restores focus', async () => {
  const open = ref(false);
  const busy = ref(false);
  const close = vi.fn(() => { open.value = false; });
  mount({ setup: () => () => h('main', [
    h('button', { id: 'trigger', onClick: () => { open.value = true; } }, 'open'),
    open.value ? h(EditorDialog, { title: 'test', busy: busy.value, onClose: close }, () => [
      h('input', { autofocus: true }),
      h('div', { style: 'display:none' }, h('button', 'hidden')),
      h('button', { id: 'last' }, 'last'),
    ]) : null,
  ]) });
  const trigger = host.querySelector<HTMLButtonElement>('#trigger')!;
  trigger.focus(); trigger.click(); await nextTick(); await nextTick();
  const dialog = host.querySelector<HTMLElement>('[role="dialog"]')!;
  host.querySelector<HTMLElement>('#last')!.focus();
  dialog.dispatchEvent(new KeyboardEvent('keydown', { key: 'Tab', bubbles: true, cancelable: true }));
  expect(document.activeElement?.getAttribute('aria-label')).toBe('关闭对话框');
  busy.value = true; await nextTick();
  dialog.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }));
  expect(close).not.toHaveBeenCalled();
  busy.value = false; await nextTick();
  dialog.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }));
  await nextTick(); await nextTick();
  expect(document.activeElement).toBe(trigger);
});

test('Shift navigation keeps its initial anchor and renders the active virtual row', async () => {
  const active = ref('0');
  const range = vi.fn();
  const words = Array.from({ length: 100 }, (_, i) => ({ id: String(i), kana: String(i), pos: [], tags: [] } as unknown as WordRow));
  mount({ setup: () => () => h(WordList, { words, activeId: active.value, selected: [], loading: false,
    onSelect: (id: string) => { active.value = id; }, onRange: range }) });
  const list = host.querySelector<HTMLElement>('[role="listbox"]')!;
  for (let i = 0; i < 30; i++) {
    list.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', shiftKey: true, bubbles: true, cancelable: true }));
    await nextTick();
  }
  expect(range.mock.calls.at(-1)![0].map((word: WordRow) => word.id)).toEqual(words.slice(0, 31).map(word => word.id));
  expect(host.querySelector('#formal-30')).not.toBeNull();
  expect(list.getAttribute('aria-activedescendant')).toBe('formal-30');
});
