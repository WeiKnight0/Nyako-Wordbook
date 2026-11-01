<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import type { SelectedWord, WordRow } from '../vocabulary';
const props = defineProps<{ words: WordRow[]; activeId: string; selected: SelectedWord[]; loading: boolean; readonly?: boolean }>();
const emit = defineEmits<{ select: [id: string]; toggle: [word: WordRow]; range: [words: WordRow[]]; open: [] }>();
const viewport = ref<HTMLElement>();
const top = ref(0);
const height = 72;
const start = computed(() => Math.max(0, Math.floor(top.value / height) - 4));
const visible = computed(() => props.words.slice(start.value, start.value + 24));
let anchor: string | undefined;
watch(() => props.words[0]?.id, () => {
  anchor = undefined;
  top.value = 0;
  if (viewport.value) viewport.value.scrollTop = 0;
});
watch(() => props.activeId, id => {
  const index = props.words.findIndex(word => word.id === id);
  if (index >= 0) reveal(index);
}, { flush: 'post' });
function reveal(index: number) {
  if (!viewport.value) return;
  if (index * height < viewport.value.scrollTop || (index + 1) * height > viewport.value.scrollTop + viewport.value.clientHeight) {
    viewport.value.scrollTop = index * height;
  }
  top.value = viewport.value.scrollTop;
}
function activate(id: string) { anchor = id; emit('select', id); viewport.value!.focus(); }
function key(event: KeyboardEvent) {
  if (event.isComposing || event.repeat || (event.target as HTMLElement).matches('input,textarea,select,button')) return;
  const index = Math.max(0, props.words.findIndex(word => word.id === props.activeId));
  if (event.key === 'Enter') { event.preventDefault(); emit('open'); return; }
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
function scroll(event: Event) { top.value = (event.target as HTMLElement).scrollTop; }
</script>

<template>
  <div ref="viewport" class="formal-viewport" role="listbox" :aria-label="readonly ? '演示词条列表' : '正式词条列表'" :aria-multiselectable="!readonly" :aria-activedescendant="visible.some(word => word.id === activeId) ? `formal-${activeId}` : undefined" :aria-busy="loading" tabindex="0" @keydown="key" @scroll="scroll">
    <div :style="{ height: `${words.length * height}px`, position: 'relative' }">
      <div :style="{ transform: `translateY(${start * height}px)` }">
        <div v-for="word in visible" :id="`formal-${word.id}`" :key="word.id" class="formal-row" :class="{ selected: activeId === word.id }" role="option" :aria-selected="selected.some(item => item.id === word.id)" @click="activate(word.id)">
          <input v-if="!readonly" type="checkbox" :aria-label="`选择 ${word.kanji || word.kana}`" :checked="selected.some(item => item.id === word.id)" @click.stop @change="emit('toggle', word)" /><span v-else aria-hidden="true"></span>
          <div><strong lang="ja">{{ word.kanji || word.kana }}</strong><small lang="ja">{{ word.kana }}</small></div>
          <div class="row-meaning">{{ word.meaning }}<small v-if="word.sense_count > 1">另有 {{ word.sense_count - 1 }} 项</small></div>
          <div class="row-meta"><small>{{ word.jlpt || 'JLPT 未知' }} · {{ word.card_state || '无卡片' }}</small><small>{{ word.pos.join(' / ') }}</small><small>{{ word.suspended ? '已暂停 · ' : '' }}{{ word.buried_until ? '已埋藏 · ' : '' }}{{ word.due_at ? new Date(word.due_at).toLocaleString() : '未安排复习' }}</small></div>
        </div>
      </div>
    </div>
    <p v-if="!words.length && !loading" class="empty">{{ readonly ? '没有匹配的演示词条，请调整筛选条件。' : '没有词条。可新增词条或清除筛选条件。' }}</p>
  </div>
</template>
