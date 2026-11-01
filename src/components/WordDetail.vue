<script setup lang="ts">
import { nextTick, onBeforeUnmount } from 'vue';
import { partsOfSpeech, type Library, type Tag, type WordDetail } from '../vocabulary';
defineProps<{ word: WordDetail; libraries: Library[]; tags: Tag[] }>();
const emit = defineEmits<{ edit: []; close: [] }>();
let restoreFocus = false;
function close(event?: KeyboardEvent) {
  if (event && (event.isComposing || event.repeat)) return;
  event?.preventDefault();
  restoreFocus = true;
  emit('close');
}
onBeforeUnmount(() => {
  if (restoreFocus) void nextTick(() => document.querySelector<HTMLElement>('[aria-label="正式词条列表"]')?.focus());
});
</script>
<template>
  <aside class="formal-detail" aria-label="词条详情" tabindex="-1" @keydown.esc="close">
    <div class="panel-heading"><h2 lang="ja">{{ word.content.kanji || word.content.kana }}</h2><button class="text-button" @click="close()">收起详情</button></div>
    <p lang="ja">{{ word.content.kana }}</p><button @click="$emit('edit')">编辑词条</button>
    <p class="muted">版本 {{ word.revision }} · {{ word.content.jlpt || 'JLPT 未知' }}</p>
    <p>{{ partsOfSpeech.filter(([code]) => word.content.pos.includes(code)).map(([, label]) => label).join(' / ') }}</p>
    <h3>全部义项</h3><ol><li v-for="sense in word.content.senses" :key="sense.id">{{ sense.meaning }}</li></ol>
    <h3>例句</h3><div v-for="example in word.content.examples" :key="example.id" class="detail-example"><p lang="ja">{{ example.jp }}</p><p>{{ example.cn }}</p><small>{{ word.content.senses.find(sense => sense.id === example.sense_id)?.meaning || '不关联义项' }}</small></div>
    <h3>音调</h3><p v-if="!word.content.pitches.length">未知</p><p v-for="pitch in word.content.pitches" :key="pitch.id">{{ pitch.mora_count }} 拍 · {{ pitch.downstep === 0 ? '平板型 0' : `下降位置 ${pitch.downstep}` }} {{ pitch.source }}</p>
    <h3>标签</h3><p>{{ tags.filter(tag => word.content.tag_ids.includes(tag.id)).map(tag => tag.name).join(' / ') || '无标签' }}</p>
    <h3>所属词库</h3><p>{{ libraries.filter(library => word.content.library_ids.includes(library.id)).map(library => library.name).join(' / ') || '未分类' }}</p>
    <h3>备注</h3><p class="plain-note">{{ word.content.note || '无备注' }}</p>
    <p class="muted">{{ word.card_state }} · {{ word.due_at ? new Date(word.due_at).toLocaleString() : '尚未安排复习' }}{{ word.suspended ? ' · 已暂停' : '' }}{{ word.buried_until ? ' · 已埋藏' : '' }}。正式评分将在 M2 开放。</p>
  </aside>
</template>
