<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { desktop, errorMessage } from '../api';
import { useVocabulary } from '../composables/useVocabulary';
import { partsOfSpeech, vocabulary, type DeleteImpact, type Library, type Tag, type WordDetail, type WordRow } from '../vocabulary';
import StoragePanel from '../components/StoragePanel.vue';
import LibraryEditor from '../components/LibraryEditor.vue';
import WordEditor from '../components/WordEditor.vue';
import WordList from '../components/WordList.vue';
import WordDetailView from '../components/WordDetail.vue';
import EditorDialog from '../components/EditorDialog.vue';

const emit = defineEmits<{ diagnostics: []; data: [selected?: string[]] }>();
const state = useVocabulary();
const {
  libraries, tags, storage, error, notice, loading, words, detail, activeId,
  selected, total, cursor, query, ready, select, search, schedule,
  beginComposition, endComposition, toggle, range, refresh, initialize,
} = state;
const libraryDialog = ref(false);
const editingLibrary = ref<Library>();
const wordDialog = ref(false);
const editingWord = ref<WordDetail>();
const editor = ref<InstanceType<typeof WordEditor>>();
const libraryEditor = ref<InstanceType<typeof LibraryEditor>>();
const showDetail = ref(window.innerWidth >= 1200);
let wideLayout = window.innerWidth >= 1200;
function resize() {
  const wide = window.innerWidth >= 1200;
  if (wide !== wideLayout) showDetail.value = wide;
  wideLayout = wide;
}
function closeConfirmation() { if (!busy.value) confirmation.value = undefined; }
function closeTag() { if (!busy.value) tagEdit.value = undefined; }
const searchInput = ref<HTMLInputElement>();
const busy = ref(false);
const batchTarget = ref('');
const tagTarget = ref('');
const tagEdit = ref<Tag>();
const tagName = ref('');
const tagSearch = ref('');
const visibleTags = computed(() => tags.value.filter(tag => tag.name.includes(tagSearch.value.trim().normalize('NFC'))));
const confirmation = ref<{ title: string; message: string; run: () => Promise<void>; second: boolean; stage: number }>();
const currentLibrary = computed(() => libraries.value.find(library => library.id === query.value.library_id));
const modal = computed(() => libraryDialog.value || wordDialog.value || Boolean(tagEdit.value) || Boolean(confirmation.value));
let unlisten: (() => void) | undefined;

async function action(run: () => Promise<void>) {
  if (busy.value) return;
  busy.value = true; error.value = '';
  try { await run(); }
  catch (cause) { error.value = errorMessage(cause); }
  finally { busy.value = false; }
}
function scope(id: string | null, unclassified = false) {
  query.value.library_id = id; query.value.unclassified = unclassified; schedule();
}
function newWord() { if (!ready.value || modal.value) return; editingWord.value = undefined; wordDialog.value = true; }
function editWord() { if (!detail.value || modal.value) return; editingWord.value = detail.value; wordDialog.value = true; }
function libraryForm(library?: Library) { if (modal.value) return; editingLibrary.value = library; libraryDialog.value = true; }
const exitRequested = ref(false);
async function closeEditor() {
  wordDialog.value = false;
  libraryDialog.value = false;
  if (!exitRequested.value) return;
  try { await getCurrentWindow().destroy(); }
  catch (cause) { exitRequested.value = false; error.value = errorMessage(cause); }
}
async function savedWord(word: WordDetail) {
  activeId.value = word.content.id; selected.value = [];
  await closeEditor();
  if (!exitRequested.value) await action(async () => { await refresh(); await select(word.content.id); });
}
async function savedLibrary() {
  await closeEditor();
  if (!exitRequested.value) await action(refresh);
}
async function batch(kind: string, target: string | null) {
  await action(async () => {
    await vocabulary<number>({ operation: 'batch', selected: [...selected.value], action: kind, target });
    selected.value = []; await refresh(); notice.value = '已保存所选词条的修改。';
  });
}
function deleteLibrary() {
  const library = currentLibrary.value!;
  confirmation.value = { title: '删除词库', message: `删除“${library.name}”及其 ${library.word_count} 条成员关系。词条、卡片和学习历史保留，未归属词条可在未分类找到。`, second: false, stage: 1, run: async () => {
    await vocabulary({ operation: 'delete_library', id: library.id, revision: library.revision });
    query.value.library_id = null; selected.value = []; await refresh();
  } };
}
async function deleteWords() {
  await action(async () => {
    const selection = [...selected.value];
    const impact = await vocabulary<DeleteImpact>({ operation: 'delete_impact', selected: selection });
    confirmation.value = { title: '永久删除词条', message: `将永久删除 ${selection.length} 个词条、${impact.memberships} 条成员关系、${impact.cards} 张卡片及 ${impact.events} 条历史事件。建议先到数据管理创建完整备份。此操作不可撤销。`, second: true, stage: 1, run: async () => {
      await vocabulary({ operation: 'batch', selected: selection, action: 'delete', target: null }); selected.value = []; await refresh();
    } };
  });
}
async function confirm() {
  const current = confirmation.value!;
  if (current.second && current.stage === 1) { current.stage = 2; return; }
  await action(async () => { await current.run(); confirmation.value = undefined; });
}
function editTag(tag: Tag) { tagEdit.value = { ...tag }; tagName.value = tag.name; }
async function saveTag() {
  await action(async () => {
    await vocabulary({ operation: 'save_tag', id: tagEdit.value!.id, name: tagName.value, expected: tagEdit.value!.name });
    tagEdit.value = undefined; selected.value = []; await refresh();
  });
}
function deleteTag() {
  const tag = tagEdit.value!;
  tagEdit.value = undefined;
  confirmation.value = { title: '删除标签', message: `删除标签“${tag.name}”及其关联，保留所有词条和卡片。`, second: false, stage: 1, run: async () => {
    await vocabulary({ operation: 'delete_tag', id: tag.id, expected: tag.name });
    query.value.tag_ids = query.value.tag_ids.filter(id => id !== tag.id); selected.value = []; await refresh();
  } };
}
function shortcuts(event: KeyboardEvent) {
  if (event.isComposing || event.repeat || modal.value || !(event.ctrlKey || event.metaKey)) return;
  if (event.key.toLowerCase() === 'f') { event.preventDefault(); searchInput.value?.focus(); }
  if (event.key.toLowerCase() === 'n') { event.preventDefault(); newWord(); }
}
onMounted(async () => {
  window.addEventListener('keydown', shortcuts);
  window.addEventListener('resize', resize);
  if (!desktop) { error.value = '请使用 npm run desktop:dev 启动原生后端，浏览器预览不提供模拟业务数据。'; return; }
  await initialize();
  try {
    const stop = await getCurrentWindow().onCloseRequested(event => {
      if (!(modal.value || busy.value)) return;
      event.preventDefault();
      if (wordDialog.value || libraryDialog.value) exitRequested.value = true;
      if (wordDialog.value) editor.value?.requestClose();
      if (libraryDialog.value) libraryEditor.value?.requestClose();
      notice.value = '请先保存或关闭编辑表单，再退出应用。';
    });
    if (disposed) stop();
    else unlisten = stop;
    if (!disposed && ready.value) {
      const { measureAcceptance } = await import('../acceptanceMeasurement');
      await measureAcceptance(state);
    }
  } catch (cause) { error.value = errorMessage(cause); }
});
let disposed = false;
onBeforeUnmount(() => {
  disposed = true;
  window.removeEventListener('keydown', shortcuts);
  window.removeEventListener('resize', resize);
  unlisten?.();
});
defineExpose({ modal });
</script>

<template>
  <div class="shell formal-shell">
    <aside class="sidebar" :inert="modal">
      <div class="brand"><span class="brand-icon">単</span><div>Nyako<span class="brand-sub">WORDBOOK</span></div></div>
      <div class="section-label">真实词汇管理 · M1-B</div>
      <button class="nav" @click="scope(null)">全部词条</button><button class="nav" @click="scope(null, true)">未分类</button>
      <div class="library-nav"><div v-for="library in libraries" :key="library.id" class="library-nav-row"><button :class="{ active: query.library_id === library.id }" @click="scope(library.id)">{{ library.name }} <small>{{ library.word_count }}{{ library.archived ? ' · 归档' : '' }}</small></button><button :aria-label="`编辑词库 ${library.name}`" class="text-button" @click="libraryForm(library)">编辑</button></div></div>
      <button :disabled="!ready" @click="libraryForm()">新建词库</button>
      <div class="sidebar-note">全部内容保存在本机。<p>文件导入、导出与备份请打开数据管理。</p></div>
      <button class="text-button" :disabled="!ready || busy || modal" @click="emit('data')">数据管理</button>
      <button class="text-button" @click="emit('diagnostics')">技术诊断</button>
    </aside>
    <main :inert="modal">
      <header><div><div class="eyebrow">词库 / 正式数据</div><h1 tabindex="-1">{{ query.unclassified ? '未分类' : currentLibrary?.name || '全部词条' }}</h1><p class="muted">{{ currentLibrary?.description || '同一词条可属于多个词库，内容与卡片保持共享。' }}</p></div><div class="toolbar"><button :disabled="!ready || busy" @click="newWord">新增词条</button><button v-if="currentLibrary" class="danger-button" :disabled="busy" @click="deleteLibrary">删除词库</button></div></header>
      <p v-if="error" class="error" role="alert">{{ error }}</p><p v-if="notice" class="notice" role="status">{{ notice }}</p>
      <StoragePanel v-if="!ready" :storage="storage" />
      <section v-if="ready" class="formal-controls" aria-label="检索筛选">
        <label class="search"><span>检索</span><input ref="searchInput" v-model="query.text" aria-label="搜索真实词条" placeholder="写法、假名、中文义项、备注" @input="schedule" @compositionstart="beginComposition" @compositionend="endComposition" /><kbd>Ctrl / ⌘ F</kbd></label>
        <div class="filters">
          <label>JLPT<select v-model="query.jlpt" @change="schedule"><option :value="null">全部</option><option v-for="level in ['N5','N4','N3','N2','N1']" :key="level">{{ level }}</option></select></label>
          <label>词性<select v-model="query.pos" @change="schedule"><option :value="null">全部</option><option v-for="[code, label] in partsOfSpeech" :key="code" :value="code">{{ label }}</option></select></label>
          <label>卡片状态<select v-model="query.state" @change="schedule"><option :value="null">全部</option><option v-for="state in ['new','learning','review','relearning']" :key="state">{{ state }}</option></select></label>
          <label>到期<select v-model="query.due" @change="schedule"><option :value="null">全部</option><option :value="true">已到期</option><option :value="false">未到期或未安排</option></select></label>
          <label>排序<select v-model="query.sort" @change="schedule"><option value="updated_at">更新时间</option><option value="kanji">写法</option><option value="kana">读音</option><option value="due_at">下次复习</option></select></label>
          <label>方向<select v-model="query.descending" @change="schedule"><option :value="true">降序</option><option :value="false">升序</option></select></label>
          <label>标签匹配<select v-model="query.tags_all" @change="schedule"><option :value="true">全部标签</option><option :value="false">任一标签</option></select></label>
        </div>
        <label>查找标签<input v-model="tagSearch" aria-label="查找标签" /></label>
        <div class="tag-filters"><label v-for="tag in visibleTags" :key="tag.id" class="inline-label"><input v-model="query.tag_ids" type="checkbox" :value="tag.id" @change="schedule" />{{ tag.name }}<button class="text-button" @click="editTag(tag)">管理</button></label></div>
        <div class="filter-chips">
          <button v-if="query.text" class="text-button" @click="query.text = ''; schedule()">关键词：{{ query.text }} ×</button>
          <button v-if="query.jlpt" class="text-button" @click="query.jlpt = null; schedule()">{{ query.jlpt }} ×</button>
          <button v-if="query.pos" class="text-button" @click="query.pos = null; schedule()">{{ query.pos }} ×</button>
          <button v-if="query.state" class="text-button" @click="query.state = null; schedule()">状态：{{ query.state }} ×</button>
          <button v-if="query.due !== null" class="text-button" @click="query.due = null; schedule()">{{ query.due ? '已到期' : '未到期或未安排' }} ×</button>
          <button v-for="id in query.tag_ids" :key="id" class="text-button" @click="query.tag_ids = query.tag_ids.filter(value => value !== id); schedule()">标签：{{ tags.find(tag => tag.id === id)?.name || id }} ×</button>
          <button class="text-button" @click="query.text = ''; query.tag_ids = []; query.jlpt = null; query.pos = null; query.state = null; query.due = null; schedule()">清除筛选</button>
        </div>
      </section>
      <section v-if="selected.length" class="batch-toolbar" aria-label="批量操作">
        <strong>当前已选 {{ selected.length }} 条</strong>
        <button :disabled="busy" @click="emit('data', selected.map(word => word.id))">导出所选词条</button>
        <select v-model="batchTarget" aria-label="目标词库"><option value="">选择词库</option><option v-for="library in libraries" :key="library.id" :value="library.id">{{ library.name }}</option></select>
        <button :disabled="busy || !batchTarget" @click="batch('add_library', batchTarget)">加入词库</button><button :disabled="busy || !batchTarget" @click="batch('remove_library', batchTarget)">移出词库</button>
        <select v-model="tagTarget" aria-label="目标标签"><option value="">选择标签</option><option v-for="tag in tags" :key="tag.id" :value="tag.id">{{ tag.name }}</option></select>
        <button :disabled="busy || !tagTarget" @click="batch('add_tag', tagTarget)">添加标签</button><button :disabled="busy || !tagTarget" @click="batch('remove_tag', tagTarget)">移除标签</button>
        <button class="danger-button" :disabled="busy" @click="deleteWords">永久删除词条…</button><button class="text-button" @click="selected = []">清空选择</button>
      </section>
      <section v-if="ready" class="formal-workspace" :class="{ 'detail-hidden': !showDetail }">
        <div class="list-panel"><div class="panel-heading"><h2>{{ total }} 个匹配词条</h2><button class="text-button" @click="showDetail = !showDetail">{{ showDetail ? '折叠详情' : '展开详情' }}</button></div>
          <WordList :words="words" :active-id="activeId" :selected="selected" :loading="loading" @select="select" @toggle="toggle" @range="range" @open="showDetail = true" />
          <footer><span>{{ loading ? '正在更新…' : `已加载 ${words.length} / ${total} 条` }}</span><button v-if="cursor" :disabled="loading" @click="search(true)">加载下一页</button></footer>
        </div>
        <WordDetailView v-if="detail && showDetail" :word="detail" :libraries="libraries" :tags="tags" @edit="editWord" @close="showDetail = false" />
      </section>
    </main>
    <LibraryEditor v-if="libraryDialog" ref="libraryEditor" :library="editingLibrary" @close="closeEditor" @saved="savedLibrary" @continue-editing="exitRequested = false" />
    <WordEditor v-if="wordDialog" ref="editor" :word="editingWord" :libraries="libraries" :tags="tags" :library-id="query.library_id" @close="closeEditor" @saved="savedWord" @tags-changed="action(refresh)" @continue-editing="exitRequested = false" />
    <EditorDialog v-if="tagEdit" title="管理标签" :busy="busy" @close="closeTag"><form class="editor-form" @submit.prevent="saveTag"><p v-if="error" class="error" role="alert">{{ error }}</p><label>标签名称<input v-model="tagName" autofocus /></label><button :disabled="busy">保存标签</button><button type="button" class="danger-button" :disabled="busy" @click="deleteTag">删除标签…</button></form></EditorDialog>
    <EditorDialog v-if="confirmation" :title="confirmation.title" :busy="busy" @close="closeConfirmation"><div class="editor-form"><p v-if="error" class="error" role="alert">{{ error }}</p><p>{{ confirmation.message }}</p><p v-if="confirmation.stage === 2" class="error">第二次确认：此操作将永久删除上述数据。</p><button class="danger-button" :disabled="busy" @click="confirm">{{ confirmation.second && confirmation.stage === 1 ? '我已了解，继续确认' : '确认执行' }}</button><button :disabled="busy" @click="confirmation = undefined">取消</button></div></EditorDialog>
  </div>
</template>
