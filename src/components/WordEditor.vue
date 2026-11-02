<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue';
import { errorMessage } from '../api';
import { moraHint, otherPartsOfSpeech, verbForms, vocabulary, type Library, type WordDetail, type WordInput, type WordPage } from '../vocabulary';
import EditorDialog from './EditorDialog.vue';

const crypto = window.crypto;
const props = defineProps<{ word?: WordDetail; libraries: Library[]; libraryId: string | null }>();
const emit = defineEmits<{ saved: [word: WordDetail]; close: []; continueEditing: [] }>();
const fresh = (): WordInput => ({ id: crypto.randomUUID(), kanji: '', kana: '', jlpt: null, note: '', senses: [{ id: crypto.randomUUID(), meaning: '' }], examples: [], pitches: [], pos: [], library_ids: props.libraryId ? [props.libraryId] : [] });
const draft = ref<WordInput>(props.word ? JSON.parse(JSON.stringify(props.word.content)) : fresh());
const original = ref(JSON.stringify(draft.value));
const revision = ref<number | null>(props.word?.revision ?? null);
const dirty = computed(() => JSON.stringify(draft.value) !== original.value);
const hasVerbForm = computed(() => draft.value.pos.some(code => verbForms.some(([form]) => form === code)));
watch(hasVerbForm, active => { if (!active) draft.value.pos = draft.value.pos.filter(item => !['intransitive', 'transitive'].includes(item)); });
const transitivity = computed({
  get: () => draft.value.pos.find(code => ['intransitive', 'transitive'].includes(code)) || '',
  set: (code: string) => { draft.value.pos = draft.value.pos.filter(item => !['intransitive', 'transitive'].includes(item)); if (code) draft.value.pos.push(code); },
});
const saving = ref(false);
const error = ref('');
const leaving = ref(false);
const latest = ref<WordDetail>();
const copied = ref(false);
const candidates = ref<string[]>([]);
let candidateSequence = 0;
let candidateTimer: ReturnType<typeof setTimeout> | undefined;
let composing = false;
function scheduleCandidates() {
  ++candidateSequence;
  clearTimeout(candidateTimer);
  candidates.value = [];
  if (!composing && !props.word && draft.value.kana.trim()) {
    candidateTimer = setTimeout(() => void findCandidates(candidateSequence), 200);
  }
}
function beginComposition() { composing = true; scheduleCandidates(); }
function endComposition() { composing = false; scheduleCandidates(); }
watch(() => [draft.value.kana, draft.value.kanji] as const, scheduleCandidates);
onBeforeUnmount(() => { ++candidateSequence; clearTimeout(candidateTimer); });
async function findCandidates(request: number) {
  const kana = draft.value.kana;
  try {
    const matches: string[] = [];
    let cursor: string | null = null;
    do {
      const page: WordPage = await vocabulary<WordPage>({ operation: 'list_words', query: { text: kana, library_id: null, unclassified: false, jlpt: null, pos: null, state: null, due: null, sort: 'kana', descending: false }, cursor, limit: 100 });
      if (request !== candidateSequence) return;
      matches.push(...page.words.filter(w => w.kana === kana.trim().normalize('NFC') && w.kanji === draft.value.kanji.trim().normalize('NFC')).map(w => w.meaning));
      cursor = page.next_cursor;
    } while (cursor);
    candidates.value = matches;
  } catch (cause) {
    if (request === candidateSequence) { candidates.value = []; error.value = `重复候选检查失败：${errorMessage(cause)}`; }
  }
}
async function save(close = false) {
  if (saving.value) return;
  if (!draft.value.kanji.trim() || !draft.value.kana.trim() || draft.value.senses.some(sense => !sense.meaning.trim())) {
    error.value = '写法、假名和释义不得为空。'; return;
  }
  saving.value = true; error.value = ''; latest.value = undefined;
  try {
    const saved = await vocabulary<WordDetail>({ operation: 'save_word', content: JSON.parse(JSON.stringify(draft.value)), expected: revision.value });
    original.value = JSON.stringify(saved.content); emit('saved', saved);
    if (close) leaving.value = false;
  } catch (cause) {
    error.value = errorMessage(cause);
    try {
      latest.value = await vocabulary<WordDetail>({ operation: 'get_word', id: draft.value.id });
      error.value += '；已按词条标识核对数据库，请比较数据库内容后决定是否重载。';
    } catch (readError) {
      error.value += `；保存结果核对：${errorMessage(readError)}`;
    }
  } finally { saving.value = false; }
}
function requestClose() {
  if (saving.value) return;
  if (dirty.value) leaving.value = true;
  else emit('close');
}
function reload() {
  draft.value = JSON.parse(JSON.stringify(latest.value!.content));
  revision.value = latest.value!.revision;
  original.value = JSON.stringify(draft.value);
  latest.value = undefined;
}
async function copy() {
  try { await navigator.clipboard.writeText(JSON.stringify(draft.value, null, 2)); copied.value = true; }
  catch (cause) { error.value = errorMessage(cause); }
}
function removeSense(index: number) {
  const id = draft.value.senses[index]!.id;
  draft.value.examples.forEach(example => { if (example.sense_id === id) example.sense_id = null; });
  draft.value.senses.splice(index, 1);
}
function move<T>(items: T[], index: number, delta: number) {
  if (index + delta < 0 || index + delta >= items.length) return;
  items.splice(index + delta, 0, items.splice(index, 1)[0]!);
}
function reorder(index: number, delta: number, collection: 'senses' | 'examples' = 'senses') {
  if (collection === 'senses') move(draft.value.senses, index, delta);
  else move(draft.value.examples, index, delta);
}
function shortcut(event: KeyboardEvent) {
  if (event.key !== 'Enter') return;
  if (event.isComposing || event.repeat) { event.preventDefault(); return; }
  if (!(event.ctrlKey || event.metaKey)) return;
  event.preventDefault(); void save();
}
defineExpose({ requestClose, saving });
</script>

<template>
  <EditorDialog :title="word ? '编辑词条' : '新增词条'" :busy="saving" @close="requestClose">
    <div v-if="leaving" class="dialog-decision"><p>词条有未保存修改，离开后草稿会丢失。</p><p v-if="error" class="error" role="alert">{{ error }}</p><div class="form-actions"><button :disabled="saving" @click="save(true)">保存并离开</button><button class="danger-button" :disabled="saving" @click="emit('close')">放弃修改</button><button class="secondary-button" :disabled="saving" @click="leaving = false; emit('continueEditing')">继续编辑</button></div></div>
    <form v-else class="editor-form" @submit.prevent="save()" @keydown="shortcut" @compositionstart="beginComposition" @compositionend="endComposition">
      <p v-if="error" class="error" role="alert">{{ error }}</p>
      <div class="form-columns">
        <label>写法（必填）<input v-model="draft.kanji" lang="ja" required autofocus /></label>
        <label>假名（必填）<input v-model="draft.kana" lang="ja" required /></label>
        <label>JLPT<select v-model="draft.jlpt"><option :value="null">未知</option><option v-for="level in ['N5','N4','N3','N2','N1']" :key="level">{{ level }}</option></select></label>
      </div>
      <p v-if="candidates.length" class="unsaved">存在同写法同读音候选：{{ candidates.join('；') }}。仍可保存为独立词条。</p>
      <fieldset><legend>中文义项</legend>
        <div v-for="(sense, index) in draft.senses" :key="sense.id" class="collection-row">
          <textarea v-model="sense.meaning" :aria-label="`义项 ${index + 1}`" required rows="2" />
          <div class="row-actions"><button type="button" class="secondary-button" :aria-label="`上移义项 ${index + 1}`" title="上移" :disabled="index === 0" @click="reorder(index, -1)">↑</button>
          <button type="button" class="secondary-button" :aria-label="`下移义项 ${index + 1}`" title="下移" :disabled="index === draft.senses.length - 1" @click="reorder(index, 1)">↓</button>
          <button type="button" class="danger-button" title="删除义项" @click="removeSense(index)">删除义项</button></div>
        </div>
        <button type="button" @click="draft.senses.push({ id: crypto.randomUUID(), meaning: '' })">新增义项</button>
      </fieldset>
      <fieldset class="grammar-fields"><legend>词性与动词属性</legend>
        <div role="group" aria-label="动词活用类别"><span class="grammar-caption">动词活用类别</span><label v-for="[code, label] in verbForms" :key="code" class="inline-label"><input v-model="draft.pos" type="checkbox" :value="code" />{{ label }}</label></div>
        <div v-if="hasVerbForm" class="transitivity-options" role="group" aria-label="自他动词"><span>自他动词</span><label><input v-model="transitivity" type="radio" value="" />未标注</label><label><input v-model="transitivity" type="radio" value="intransitive" />自动词</label><label><input v-model="transitivity" type="radio" value="transitive" />他动词</label></div>
        <div class="other-pos"><label v-for="[code, label] in otherPartsOfSpeech" :key="code" class="inline-label"><input v-model="draft.pos" type="checkbox" :value="code" />{{ label }}</label></div>
      </fieldset>
      <details class="editor-more" :open="Boolean(draft.examples.length)"><summary>例句{{ draft.examples.length ? `（${draft.examples.length}）` : '' }}</summary><fieldset><legend>例句</legend>
        <div v-for="(example, index) in draft.examples" :key="example.id" class="example-fields">
          <label>日文例句<input v-model="example.jp" lang="ja" required /></label><label>中文翻译（可空）<input v-model="example.cn" lang="zh-CN" /></label>
          <label>关联义项<select v-model="example.sense_id"><option :value="null">不关联义项</option><option v-for="(sense, position) in draft.senses" :key="sense.id" :value="sense.id">{{ position + 1 }} · {{ sense.meaning }}</option></select></label>
          <div class="row-actions"><button type="button" class="secondary-button" :aria-label="`上移例句 ${index + 1}`" title="上移" :disabled="index === 0" @click="reorder(index, -1, 'examples')">↑</button>
          <button type="button" class="secondary-button" :aria-label="`下移例句 ${index + 1}`" title="下移" :disabled="index === draft.examples.length - 1" @click="reorder(index, 1, 'examples')">↓</button>
          <button type="button" class="danger-button" @click="draft.examples.splice(index, 1)">删除例句</button></div>
        </div>
        <button type="button" @click="draft.examples.push({ id: crypto.randomUUID(), sense_id: null, jp: '', cn: null })">新增例句</button>
      </fieldset></details>
      <details class="editor-more" :open="Boolean(draft.pitches.length)"><summary>音调{{ draft.pitches.length ? `（${draft.pitches.length}）` : '' }}</summary><fieldset><legend>音调（0 为平板型）</legend>
        <p class="muted">读音拍数：{{ moraHint(draft.kana) }}</p>
        <div v-for="(pitch, index) in draft.pitches" :key="pitch.id" class="collection-row pitch-row">
          <label>音调<input v-model.number="pitch.downstep" type="number" min="0" :max="moraHint(draft.kana)" required /></label>
          <div class="row-actions"><button type="button" class="danger-button" @click="draft.pitches.splice(index, 1)">删除音调</button></div>
        </div>
        <button type="button" @click="draft.pitches.push({ id: crypto.randomUUID(), downstep: 0 })">新增音调</button>
      </fieldset></details>
      <details class="editor-more" :open="Boolean(draft.library_ids.length)"><summary>所属词库{{ draft.library_ids.length ? `（${draft.library_ids.length}）` : '' }}</summary><fieldset><legend>所属词库</legend><label v-for="library in libraries" :key="library.id" class="inline-label"><input v-model="draft.library_ids" type="checkbox" :value="library.id" />{{ library.name }}{{ library.archived ? '（归档）' : '' }}</label></fieldset></details>
      <details class="editor-more" :open="Boolean(draft.note)"><summary>备注{{ draft.note ? '（已填写）' : '' }}</summary><label>备注<textarea v-model="draft.note" rows="4" /></label></details>
      <section v-if="latest" class="conflict-panel" role="alert">
        <h3>数据库内容核对：本地草稿已保留</h3><div class="form-columns"><div><h4>当前数据库</h4><pre>{{ JSON.stringify(latest.content, null, 2) }}</pre></div><div><h4>本地草稿</h4><pre>{{ JSON.stringify(draft, null, 2) }}</pre></div></div>
        <button type="button" @click="copy">{{ copied ? '已复制草稿' : '复制本地草稿' }}</button><button type="button" @click="reload">放弃本地修改并重载最新版本</button>
      </section>
      <div class="form-actions"><button :disabled="saving">{{ saving ? '保存中…' : '保存词条' }}</button><button type="button" @click="requestClose">取消</button></div>
    </form>
  </EditorDialog>
</template>
