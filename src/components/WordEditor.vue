<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue';
import { errorMessage } from '../api';
import { otherPartsOfSpeech, verbForms, vocabulary, type DeleteImpact, type Library, type WordDetail, type WordInput, type WordPage } from '../vocabulary';
import EditorDialog from './EditorDialog.vue';

const crypto = window.crypto;
const props = defineProps<{ word?: WordDetail; libraries: Library[]; libraryId: string | null }>();
const emit = defineEmits<{ saved: [word: WordDetail]; close: []; deleted: []; continueEditing: [] }>();
const fresh = (): WordInput => ({ id: crypto.randomUUID(), kanji: '', kana: '', jlpt: null, note: '', meaning: '', pitch: '', examples_jp: '', examples_cn: '', pos: [], library_ids: props.libraryId ? [props.libraryId] : [] });
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
  if (!draft.value.kanji.trim() || !draft.value.kana.trim() || !draft.value.meaning.trim()) {
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
  if (deletion.value) { deletion.value = undefined; return; }
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
const deletion = ref<DeleteImpact>();
async function requestDelete() {
  if (!props.word || saving.value) return;
  saving.value = true; error.value = '';
  try { deletion.value = await vocabulary<DeleteImpact>({ operation: 'delete_impact', selected: [{ id: props.word.content.id, revision: revision.value! }] }); }
  catch (cause) { error.value = errorMessage(cause); }
  finally { saving.value = false; }
}
async function deleteWord() {
  if (!props.word || saving.value) return;
  saving.value = true; error.value = '';
  try {
    await vocabulary({ operation: 'batch', selected: [{ id: props.word.content.id, revision: revision.value! }], action: 'delete', target: null });
    emit('deleted');
  } catch (cause) { error.value = errorMessage(cause); }
  finally { saving.value = false; }
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
    <div v-if="deletion" class="dialog-decision"><h3>删除“{{ word!.content.kanji }}”？</h3><p>将永久删除此词条、{{ deletion.memberships }} 条词库关系、{{ deletion.cards }} 张卡片和 {{ deletion.events }} 条学习记录，无法撤销。</p><p v-if="dirty">未保存的修改也会丢弃。</p><p v-if="error" class="error" role="alert">{{ error }}</p><div class="form-actions"><button class="danger-button" :disabled="saving" @click="deleteWord">确认删除</button><button class="secondary-button" :disabled="saving" @click="deletion = undefined">继续编辑</button></div></div>
    <div v-else-if="leaving" class="dialog-decision"><p>词条有未保存修改，离开后草稿会丢失。</p><p v-if="error" class="error" role="alert">{{ error }}</p><div class="form-actions"><button :disabled="saving" @click="save(true)">保存并离开</button><button class="danger-button" :disabled="saving" @click="emit('close')">放弃修改</button><button class="secondary-button" :disabled="saving" @click="leaving = false; emit('continueEditing')">继续编辑</button></div></div>
    <form v-else class="editor-form" @submit.prevent="save()" @keydown="shortcut" @compositionstart="beginComposition" @compositionend="endComposition">
      <p v-if="error" class="error" role="alert">{{ error }}</p>
      <div class="form-columns">
        <label>写法（必填）<input v-model="draft.kanji" lang="ja" required autofocus /></label>
        <label>假名（必填）<input v-model="draft.kana" lang="ja" required /></label>
        <label>JLPT<select v-model="draft.jlpt"><option :value="null">未知</option><option v-for="level in ['N5','N4','N3','N2','N1']" :key="level">{{ level }}</option></select></label>
      </div>
      <p v-if="candidates.length" class="unsaved">存在同写法同读音候选：{{ candidates.join('；') }}。仍可保存为独立词条。</p>
      <label>释义（必填）<textarea v-model="draft.meaning" aria-label="释义" required rows="5" maxlength="100000" /></label>
      <label>音调<input v-model="draft.pitch" aria-label="音调" maxlength="1000" placeholder="例如：0 / 2，或平板型" /></label>
      <fieldset class="grammar-fields"><legend>词性与动词属性</legend>
        <div role="group" aria-label="动词活用类别"><span class="grammar-caption">动词活用类别</span><label v-for="[code, label] in verbForms" :key="code" class="inline-label"><input v-model="draft.pos" type="checkbox" :value="code" />{{ label }}</label></div>
        <div v-if="hasVerbForm" class="transitivity-options" role="group" aria-label="自他动词"><span>自他动词</span><label><input v-model="transitivity" type="radio" value="" />未标注</label><label><input v-model="transitivity" type="radio" value="intransitive" />自动词</label><label><input v-model="transitivity" type="radio" value="transitive" />他动词</label></div>
        <div class="other-pos"><label v-for="[code, label] in otherPartsOfSpeech" :key="code" class="inline-label"><input v-model="draft.pos" type="checkbox" :value="code" />{{ label }}</label></div>
      </fieldset>
      <label>日文例句<textarea v-model="draft.examples_jp" aria-label="日文例句" lang="ja" rows="4" maxlength="100000" /></label>
      <label>例句中文翻译<textarea v-model="draft.examples_cn" aria-label="例句中文翻译" lang="zh-CN" rows="4" maxlength="100000" /></label>
      <details class="editor-more" :open="Boolean(draft.library_ids.length)"><summary>所属词库{{ draft.library_ids.length ? `（${draft.library_ids.length}）` : '' }}</summary><fieldset><legend>所属词库</legend><label v-for="library in libraries" :key="library.id" class="inline-label"><input v-model="draft.library_ids" type="checkbox" :value="library.id" />{{ library.name }}{{ library.archived ? '（归档）' : '' }}</label></fieldset></details>
      <details class="editor-more" :open="Boolean(draft.note)"><summary>备注{{ draft.note ? '（已填写）' : '' }}</summary><label>备注<textarea v-model="draft.note" rows="4" /></label></details>
      <section v-if="latest" class="conflict-panel" role="alert">
        <h3>数据库内容核对：本地草稿已保留</h3><div class="form-columns"><div><h4>当前数据库</h4><pre>{{ JSON.stringify(latest.content, null, 2) }}</pre></div><div><h4>本地草稿</h4><pre>{{ JSON.stringify(draft, null, 2) }}</pre></div></div>
        <button type="button" @click="copy">{{ copied ? '已复制草稿' : '复制本地草稿' }}</button><button type="button" @click="reload">放弃本地修改并重载最新版本</button>
      </section>
      <div class="form-actions"><button :disabled="saving">{{ saving ? '保存中…' : '保存词条' }}</button><button type="button" :disabled="saving" @click="requestClose">取消</button><button v-if="word" type="button" class="danger-button editor-delete" :disabled="saving" @click="requestDelete">删除词条</button></div>
    </form>
  </EditorDialog>
</template>
