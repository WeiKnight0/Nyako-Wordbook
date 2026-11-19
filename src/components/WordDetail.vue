<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { posLabels, type Library, type WordDetail } from '../vocabulary';

const props = defineProps<{
  word?: WordDetail; libraries: Library[]; readonly?: boolean;
  position?: number; total?: number; hasPrevious?: boolean; hasNext?: boolean;
  navigating?: boolean; error?: string;
}>();
const emit = defineEmits<{ edit: []; close: []; navigate: [direction: -1 | 1]; retry: [] }>();
const content = ref<HTMLElement>();
watch(() => props.word?.content.id, () => { if (content.value) content.value.scrollTop = 0; }, { flush: 'post' });
const memberships = computed(() => props.libraries.filter(library => props.word?.content.library_ids.includes(library.id)).map(library => library.name).join(' / ') || '未分类');
function canNavigate(direction: -1 | 1) {
  return !props.navigating && (direction === -1 ? props.hasPrevious : props.hasNext);
}
function navigate(direction: -1 | 1, event?: KeyboardEvent) {
  if (!canNavigate(direction)) return;
  event?.preventDefault();
  emit('navigate', direction);
}
function key(event: KeyboardEvent) {
  if (event.defaultPrevented || event.isComposing || event.keyCode === 229 || event.repeat || event.ctrlKey || event.metaKey || event.altKey || event.shiftKey) return;
  if (event.key === 'Escape') { event.preventDefault(); emit('close'); return; }
  if (!['ArrowLeft', 'ArrowRight'].includes(event.key)) return;
  if ((event.target as Element).closest('input, textarea, select, button, a, details, summary, [contenteditable]:not([contenteditable="false"]), [role="button"], [role="link"], [role="textbox"], [tabindex]:not([tabindex="-1"])')) return;
  navigate(event.key === 'ArrowLeft' ? -1 : 1, event);
}
</script>

<template>
  <aside class="formal-detail" aria-label="词条详情" tabindex="-1" @keydown="key">
    <div class="detail-toolbar">
      <button type="button" class="detail-back" aria-label="收回词条详情" @click="emit('close')"><span aria-hidden="true">←</span> 返回列表</button>
      <div class="detail-navigation" aria-label="词条导航">
        <span v-if="position !== undefined && total !== undefined" class="detail-position" aria-live="polite">第 {{ position }} / {{ total }} 条</span>
        <button type="button" class="detail-nav" aria-label="上一个词条" :aria-disabled="!canNavigate(-1)" @click="navigate(-1)">上一词</button>
        <button type="button" class="detail-nav" aria-label="下一个词条" :aria-disabled="!canNavigate(1)" @click="navigate(1)">下一词</button>
      </div>
    </div>
    <div ref="content" class="detail-content" :aria-busy="navigating || (!word && !error)">
      <div v-if="error" class="detail-error" role="alert"><p>{{ error }}</p><button type="button" class="secondary-button" :disabled="navigating" @click="emit('retry')">重试</button></div>
      <p v-else-if="!word" class="detail-loading" role="status">正在加载词条…</p>
      <template v-if="word">
        <header class="detail-heading">
          <div class="detail-top"><span class="eyebrow">词条详情</span><button v-if="!readonly" type="button" class="detail-edit" :disabled="navigating" @click="emit('edit')">编辑词条</button></div>
          <h2 lang="ja">{{ word.content.kanji || word.content.kana }}</h2>
          <p v-if="word.content.kanji && word.content.kanji !== word.content.kana" class="detail-reading" lang="ja">{{ word.content.kana }}</p>
          <div v-if="word.content.jlpt || word.content.pos.length" class="detail-meta">
            <span v-if="word.content.jlpt" class="detail-chip">{{ word.content.jlpt }}</span>
            <span v-if="word.content.pos.length" class="detail-chip">{{ posLabels(word.content.pos) }}</span>
          </div>
          <div v-if="word.content.pitch" class="detail-pitch"><h3>音调</h3><p class="plain-text">{{ word.content.pitch }}</p></div>
        </header>
        <section class="detail-section" aria-label="释义"><h3>释义</h3><p class="plain-text">{{ word.content.meaning }}</p></section>
        <section v-if="word.content.examples_jp" class="detail-section" aria-label="日文例句"><h3>日文例句</h3><p class="plain-text" lang="ja">{{ word.content.examples_jp }}</p></section>
        <section v-if="word.content.examples_cn" class="detail-section" aria-label="例句中文翻译"><h3>例句中文翻译</h3><p class="plain-text" lang="zh-CN">{{ word.content.examples_cn }}</p></section>
        <footer class="detail-footer">
          <section v-if="word.content.note" class="detail-note"><h3>备注</h3><p class="plain-note" lang="zh-CN">{{ word.content.note }}</p></section>
          <section class="detail-memberships"><h3>所属词库</h3><p>{{ memberships }}</p></section>
        </footer>
      </template>
    </div>
    <div class="detail-key-hint">← → 连续浏览 <span>Esc 返回列表</span></div>
  </aside>
</template>

<style scoped>
.plain-text { white-space: pre-wrap; overflow-wrap: anywhere; line-height: 1.9; margin: 0; }
.formal-detail { display: flex; flex-direction: column; min-height: 0; color: #283b33; }
.detail-key-hint { display: flex; justify-content: space-between; gap: 12px; padding: 10px 22px; border-top: 1px solid #e7ecdf; color: #839079; background: #fafcf8; font-size: 11px; }
.detail-heading { display: block; margin: 0; }
.detail-toolbar { display: flex; flex-wrap: wrap; align-items: center; justify-content: space-between; gap: 10px; padding: 16px 22px; border-bottom: 1px solid #e2e9df; background: #f8faf6; }
.detail-toolbar button, .detail-edit { border: 0; background: transparent; box-shadow: none; color: #466553; font-size: 12px; padding: 7px 9px; border-radius: 5px; }
.detail-toolbar button:hover:not([aria-disabled=true]), .detail-edit:hover { background: #e6eee1; }
.detail-toolbar button[aria-disabled=true] { opacity: .4; cursor: default; }
.detail-back { white-space: nowrap; }
.detail-navigation { display: flex; align-items: center; flex-wrap: wrap; gap: 4px; }
.detail-position { margin-right: 8px; color: #728174; font-size: 12px; font-variant-numeric: tabular-nums; }
.detail-content { flex: 1; min-height: 0; height: auto; overflow: auto; padding: 26px clamp(20px, 3vw, 40px) 32px; overflow-wrap: anywhere; }
.detail-top { display: flex; align-items: center; justify-content: space-between; gap: 12px; }
.eyebrow { color: #7a897c; font-size: 11px; letter-spacing: .12em; }
.formal-detail h2 { margin: 16px 0 6px; font-size: clamp(34px, 3.5vw, 48px); font-weight: 600; line-height: 1.35; letter-spacing: .025em; white-space: pre-wrap; }
.detail-reading { margin: 0; color: #56725f; font-size: 21px; line-height: 1.7; white-space: pre-wrap; }
.detail-meta { display: flex; flex-wrap: wrap; gap: 7px; margin: 16px 0 0; }
.detail-chip { border: 1px solid #dce6d7; border-radius: 5px; background: #f4f7ef; padding: 3px 8px; color: #59704d; font-size: 12px; line-height: 1.6; }
.detail-pitch { margin-top: 20px; }
.detail-pitch h3, .detail-footer h3 { margin: 0 0 7px; color: #7a877b; font-size: 11px; font-weight: 500; }
.detail-pitch > p { margin: 8px 0 0; color: #7b877c; font-size: 11px; line-height: 1.7; }
.detail-section { border-top: 1px solid #e1e8dc; padding: 22px 0 0; margin-top: 24px; }
.detail-section h3 { margin: 0 0 16px; font-size: 12px; font-weight: 600; color: #61765b; letter-spacing: .05em; }
.detail-footer { margin-top: 26px; padding-top: 20px; border-top: 1px solid #e5eadf; }
.detail-footer section + section { margin-top: 17px; }
.detail-footer p { margin: 0; font-size: 12px; line-height: 1.9; color: #7b8677; white-space: pre-wrap; }
.detail-edit:disabled:disabled { opacity: .4; cursor: default; }
.detail-error { margin-bottom: 22px; padding: 16px; border: 1px solid #e7c9bd; border-radius: 6px; background: #fff8f3; color: #8e4a36; }
.detail-error p { margin: 0 0 12px; font-size: 14px; line-height: 1.8; white-space: pre-wrap; }
.detail-loading { margin: 30px 0; color: #74836e; font-size: 14px; line-height: 1.8; }
@media (max-width: 640px) {
  .detail-toolbar { padding: 12px 14px; }
  .detail-content { padding: 22px 18px 28px; }
  .detail-navigation { gap: 2px; }
  .detail-position { margin-right: 3px; }
}
</style>
