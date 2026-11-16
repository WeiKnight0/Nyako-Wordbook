<script setup lang="ts">
import { computed, onActivated, ref, watch } from 'vue';
import { posLabels, type SelectedWord, type WordRow } from '../vocabulary';
const props = defineProps<{ words: WordRow[]; activeId: string; selected: SelectedWord[]; loading: boolean; readonly?: boolean; compact?: boolean; revealActive?: boolean }>();
const emit = defineEmits<{ select: [id: string]; toggle: [word: WordRow]; range: [words: WordRow[]]; open: [id: string]; position: [index: number] }>();
const viewport = ref<HTMLElement>();
const top = ref(0);
const height = 72;
const start = computed(() => Math.max(0, Math.floor(top.value / height) - 4));
const visible = computed(() => props.words.slice(start.value, start.value + 24));
let anchor: string | undefined;
onActivated(() => { viewport.value!.scrollTop = top.value; });
watch(() => props.activeId, id => {
  if (props.revealActive) reveal(props.words.findIndex(word => word.id === id));
}, { flush: 'post' });
watch(() => props.words.map(word => word.id), (ids, previous) => {
  if (ids.length !== previous.length) {
    top.value = Math.min(top.value, Math.max(0, ids.length * height - viewport.value!.clientHeight));
    viewport.value!.scrollTop = top.value;
  }
  // 相同顺序的刷新和追加分页保留滚动；已有词目换位或移除时重新对齐活动项。
  if (!previous.some((id, index) => id !== ids[index])) return;
  anchor = undefined;
  const index = ids.indexOf(props.activeId);
  if (index >= 0) reveal(index);
}, { flush: 'post' });
function reveal(index: number) {
  if (!viewport.value) return;
  if (index * height < viewport.value.scrollTop || (index + 1) * height > viewport.value.scrollTop + viewport.value.clientHeight) {
    viewport.value.scrollTop = index * height;
  }
  top.value = viewport.value.scrollTop;
}
function activate(id: string) { anchor = id; viewport.value!.focus({ preventScroll: true }); emit('open', id); }
function key(event: KeyboardEvent) {
  if (event.isComposing || event.repeat || event.ctrlKey || event.metaKey || event.altKey || (event.target as HTMLElement).closest('input,textarea,select,button,[contenteditable="true"]')) return;
  const index = Math.max(0, props.words.findIndex(word => word.id === props.activeId));
  if (event.key === 'Enter') { event.preventDefault(); if (props.words[index]) emit('open', props.words[index]!.id); return; }
  if (event.key === ' ') { event.preventDefault(); if (!props.readonly && props.words[index]) emit('toggle', props.words[index]!); return; }
  if (!['ArrowDown', 'ArrowUp', 'j', 'k'].includes(event.key) || !props.words.length) return;
  event.preventDefault();
  const next = Math.max(0, Math.min(props.words.length - 1, index + (['ArrowDown', 'j'].includes(event.key) ? 1 : -1)));
  if (!event.shiftKey || props.readonly) anchor = props.words[next]!.id;
  else {
    anchor ??= props.words[index]!.id;
    const startIndex = Math.max(0, props.words.findIndex(word => word.id === anchor));
    emit('range', props.words.slice(Math.min(startIndex, next), Math.max(startIndex, next) + 1));
  }
  emit('select', props.words[next]!.id);
  reveal(next);
}
function scroll(event: Event) {
  top.value = (event.target as HTMLElement).scrollTop;
  emit('position', Math.floor(top.value / height));
}
function scrollToIndex(index: number) {
  viewport.value!.scrollTop = index * height;
  top.value = viewport.value!.scrollTop;
}
defineExpose({ scrollToIndex });
</script>

<template>
  <div ref="viewport" class="formal-viewport" :class="{ 'compact-list': compact }" role="listbox" :aria-label="readonly ? '演示词条列表' : '正式词条列表'" :aria-multiselectable="!readonly" :aria-activedescendant="visible.some(word => word.id === activeId) ? `formal-${activeId}` : undefined" :aria-busy="loading" tabindex="0" @keydown="key" @scroll="scroll">
    <div :style="{ height: `${words.length * height}px`, position: 'relative' }">
      <div :style="{ transform: `translateY(${start * height}px)` }">
        <div v-for="word in visible" :id="`formal-${word.id}`" :key="word.id" class="formal-row" :class="{ selected: activeId === word.id, checked: selected.some(item => item.id === word.id) }" role="option" :aria-selected="selected.some(item => item.id === word.id)" @click="activate(word.id)">
          <input v-if="!readonly" type="checkbox" :aria-label="`选择 ${word.kanji || word.kana}`" :checked="selected.some(item => item.id === word.id)" @click.stop @change="emit('toggle', word)" /><span v-else aria-hidden="true"></span>
          <div><strong lang="ja">{{ word.kanji || word.kana }}</strong><small lang="ja">{{ word.kana }}</small></div>
          <div class="row-meaning">{{ word.meaning }}<small v-if="word.sense_count > 1">另有 {{ word.sense_count - 1 }} 项</small></div>
          <div class="row-meta"><small v-if="word.jlpt">{{ word.jlpt }}</small><small v-if="word.pos.length">{{ posLabels(word.pos) }}</small></div>
        </div>
      </div>
    </div>
    <p v-if="!words.length && !loading" class="empty">{{ readonly ? '没有匹配的演示词条，请调整筛选条件。' : '没有词条。可新增词条或清除筛选条件。' }}</p>
  </div>
</template>
