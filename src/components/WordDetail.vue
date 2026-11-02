<script setup lang="ts">
import { computed } from 'vue';
import { partsOfSpeech, type Library, type WordDetail } from '../vocabulary';
const props = defineProps<{ word?: WordDetail; libraries: Library[]; readonly?: boolean }>();
const partLabels = computed(() => partsOfSpeech.filter(([code]) => props.word?.content.pos.includes(code)).map(([, label]) => label).join(' / '));
const memberships = computed(() => props.libraries.filter(library => props.word?.content.library_ids.includes(library.id)).map(library => library.name).join(' / ') || '未分类');
const emit = defineEmits<{ edit: []; delete: []; close: [] }>();
function close(event?: KeyboardEvent) {
  if (event && (event.isComposing || event.repeat)) return;
  event?.preventDefault();
  emit('close');
}
</script>
<template>
  <aside class="formal-detail" aria-label="词条详情" tabindex="-1" @keydown.esc="close">
    <button class="drawer-handle" aria-label="收回词条详情" title="收回词条详情" @click="close()">‹</button>
    <div class="detail-content" :aria-busy="!word">
      <p v-if="!word" class="detail-loading" role="status">正在加载词条…</p>
      <template v-else>
      <div class="detail-top"><span class="eyebrow">词条详情</span><div v-if="!readonly" class="detail-actions"><button class="secondary-button" @click="$emit('edit')">编辑词条</button><button class="danger-button" @click="$emit('delete')">删除词条</button></div></div>
      <h2 lang="ja">{{ word.content.kanji || word.content.kana }}</h2>
      <p class="detail-reading" lang="ja">{{ word.content.kana }}</p>
      <p class="detail-meta">{{ word.content.jlpt || 'JLPT 未知' }}<template v-if="word.content.pos.length"> · {{ partLabels }}</template></p>
      <section class="detail-section"><h3>释义</h3><ol><li v-for="sense in word.content.senses" :key="sense.id" lang="zh-CN">{{ sense.meaning }}</li></ol></section>
      <section v-if="word.content.examples.length" class="detail-section"><h3>例句</h3><div v-for="example in word.content.examples" :key="example.id" class="detail-example"><p lang="ja">{{ example.jp }}</p><p lang="zh-CN">{{ example.cn }}</p></div></section>
      <section class="detail-section detail-facts"><div><h3>音调</h3><p>{{ word.content.pitches.length ? word.content.pitches.map(pitch => pitch.downstep).join(' / ') : '未知' }}</p></div><div><h3>所属词库</h3><p>{{ memberships }}</p></div></section>
      <section v-if="word.content.note" class="detail-section"><h3>备注</h3><p class="plain-note" lang="zh-CN">{{ word.content.note }}</p></section>
      </template>
    </div>
  </aside>
</template>
