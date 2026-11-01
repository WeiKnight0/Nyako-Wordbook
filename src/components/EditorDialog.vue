<script setup lang="ts">
import { nextTick, onBeforeUnmount, onMounted, onUnmounted, ref } from 'vue';

defineProps<{ title: string; busy?: boolean }>();
const emit = defineEmits<{ close: [] }>();
const panel = ref<HTMLElement>();
const previous = document.activeElement as HTMLElement | null;
let disposed = false;
function ancestors(item: HTMLElement): HTMLElement[] {
  return item.parentElement ? [item.parentElement, ...ancestors(item.parentElement)] : [];
}
onBeforeUnmount(() => { disposed = true; });
function keydown(event: KeyboardEvent) {
  if (event.isComposing || event.repeat) return;
  if (event.key === 'Escape') { event.preventDefault(); emit('close'); return; }
  if (event.key !== 'Tab') return;
  const items = [...panel.value!.querySelectorAll<HTMLElement>('button:not(:disabled),input:not(:disabled),select:not(:disabled),textarea:not(:disabled),[tabindex="0"]')]
    .filter(item => !item.closest('[hidden],[inert]') && ![item, ...ancestors(item)].some(node => getComputedStyle(node).display === 'none' || getComputedStyle(node).visibility === 'hidden'));
  if (!items.length) { event.preventDefault(); panel.value!.focus(); return; }
  if (event.shiftKey && document.activeElement === items[0]) { event.preventDefault(); items.at(-1)!.focus(); }
  if (!event.shiftKey && document.activeElement === items.at(-1)) { event.preventDefault(); items[0]!.focus(); }
}
onMounted(async () => {
  await nextTick();
  if (!disposed) panel.value!.querySelector<HTMLElement>('[autofocus],input,button')?.focus();
});
onUnmounted(() => {
  disposed = true;
  void nextTick(() => {
    if (previous?.isConnected && !previous.closest('[inert]')) previous.focus();
    else document.querySelector<HTMLElement>('main h1')?.focus();
  });
});
</script>

<template>
  <div class="modal-backdrop">
    <section ref="panel" class="editor-dialog" role="dialog" aria-modal="true" :aria-label="title" tabindex="-1" @keydown="busy && $event.key === 'Escape' ? $event.preventDefault() : keydown($event)">
      <div class="panel-heading"><h2>{{ title }}</h2><button class="text-button" aria-label="关闭对话框" :disabled="busy" @click="emit('close')">关闭</button></div>
      <slot />
    </section>
  </div>
</template>
