<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { desktop, errorMessage } from '../api';
import { useVocabulary } from '../composables/useVocabulary';
import { partsOfSpeech, vocabulary, type DeleteImpact, type Library, type WordDetail } from '../vocabulary';
import StoragePanel from '../components/StoragePanel.vue';
import LibraryEditor from '../components/LibraryEditor.vue';
import WordEditor from '../components/WordEditor.vue';
import WordList from '../components/WordList.vue';
import WordDetailView from '../components/WordDetail.vue';
import EditorDialog from '../components/EditorDialog.vue';

const emit = defineEmits<{ diagnostics: []; data: [selected?: string[], section?: 'import' | 'export' | 'backup'] }>();
const state = useVocabulary();
const {
  libraries, storage, error, notice, loading, words, detail, detailLoading, activeId,
  selected, total, cursor, query, ready, select, search, schedule,
  beginComposition, endComposition, toggle, range, refresh, initialize,
} = state;
const libraryDialog = ref(false);
const editingLibrary = ref<Library>();
const wordDialog = ref(false);
const editingWord = ref<WordDetail>();
const editor = ref<InstanceType<typeof WordEditor>>();
const libraryEditor = ref<InstanceType<typeof LibraryEditor>>();
const showDetail = ref(false);
function openWord(id: string) { showDetail.value = true; void select(id); }
function closeDetail() { showDetail.value = false; }
function detailExited() { document.querySelector<HTMLElement>('[aria-label="正式词条列表"], [aria-label="演示词条列表"]')?.focus(); }
function closeConfirmation() { if (!busy.value) confirmation.value = undefined; }
const searchInput = ref<HTMLInputElement>();
const busy = ref(false);
const batchTarget = ref('');
const confirmation = ref<{ title: string; message: string; run: () => Promise<void>; second: boolean; stage: number }>();
const currentLibrary = computed(() => libraries.value.find(library => library.id === query.value.library_id));
const modal = computed(() => libraryDialog.value || wordDialog.value || Boolean(confirmation.value));
const demo = import.meta.env.DEV && !desktop;
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
function newWord() { if (!ready.value || modal.value || demo) return; editingWord.value = undefined; wordDialog.value = true; }
function editWord() { if (!detail.value || modal.value || demo) return; editingWord.value = detail.value; wordDialog.value = true; }
function libraryForm(library?: Library) { if (modal.value || demo) return; editingLibrary.value = library; libraryDialog.value = true; }
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
async function deleteSelection(selection: { id: string; revision: number }[]) {
  await action(async () => {
    const impact = await vocabulary<DeleteImpact>({ operation: 'delete_impact', selected: selection });
    confirmation.value = { title: '永久删除词条', message: `将永久删除 ${selection.length} 个词条、${impact.memberships} 条成员关系、${impact.cards} 张卡片及 ${impact.events} 条历史事件。建议先到数据管理创建完整备份。此操作不可撤销。`, second: true, stage: 1, run: async () => {
      await vocabulary({ operation: 'batch', selected: selection, action: 'delete', target: null });
      selected.value = []; showDetail.value = false; await refresh(); notice.value = `已删除 ${selection.length} 个词条。`;
    } };
  });
}
function deleteWords() { void deleteSelection([...selected.value]); }
function deleteCurrentWord() {
  if (!detail.value || busy.value || modal.value) return;
  void deleteSelection([{ id: detail.value.content.id, revision: detail.value.revision }]);
}
async function confirm() {
  const current = confirmation.value!;
  if (current.second && current.stage === 1) { current.stage = 2; return; }
  await action(async () => { await current.run(); confirmation.value = undefined; });
}
function shortcuts(event: KeyboardEvent) {
  if (event.isComposing || event.repeat || modal.value || !(event.ctrlKey || event.metaKey)) return;
  if (event.key.toLowerCase() === 'f') { event.preventDefault(); searchInput.value?.focus(); }
  if (event.key.toLowerCase() === 'n') { event.preventDefault(); newWord(); }
}
onMounted(async () => {
  window.addEventListener('keydown', shortcuts);
  if (!desktop && !demo) { error.value = '浏览器生产预览没有原生后端，请使用 npm run desktop:dev。'; return; }
  await initialize();
  if (!desktop) return;
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
  unlisten?.();
});
defineExpose({ modal });
</script>

<template>
  <div class="shell formal-shell">
    <aside class="sidebar" :inert="modal">
      <div class="brand"><span class="brand-icon">単</span><div>Nyako<span class="brand-sub">WORDBOOK</span></div></div>
      <div class="sidebar-group-label">词条</div>
      <button class="nav" :class="{ active: !query.library_id && !query.unclassified }" @click="scope(null)">全部词条 <span>{{ total }}</span></button>
      <button class="nav" :class="{ active: query.unclassified }" @click="scope(null, true)">未分类</button>
      <div class="sidebar-group-label library-heading">词库 <button class="text-button" :disabled="!ready || demo" @click="libraryForm()">新建词库</button></div>
      <div class="library-nav"><div v-for="library in libraries" :key="library.id" class="library-nav-row"><button :class="{ active: query.library_id === library.id }" @click="scope(library.id)">{{ library.name }} <small>{{ library.word_count }}{{ library.archived ? ' · 归档' : '' }}</small></button><button :aria-label="`编辑词库 ${library.name}`" class="text-button" :disabled="demo" @click="libraryForm(library)">编辑</button></div></div>
      <div class="sidebar-actions"><button class="sidebar-import" :disabled="busy || modal" @click="emit('data', [], 'import')">数据管理</button><button class="text-button" :disabled="demo" @click="emit('diagnostics')">技术诊断</button></div>
    </aside>
    <main :inert="modal">
      <header class="library-header"><div><div class="eyebrow">词汇管理 / {{ currentLibrary ? '词库' : '全部' }}</div><h1 tabindex="-1">{{ query.unclassified ? '未分类' : currentLibrary?.name || '全部词条' }}</h1><p v-if="currentLibrary?.description" class="muted">{{ currentLibrary.description }}</p></div><div class="toolbar"><button class="secondary-button" :disabled="busy" @click="emit('data', [], 'import')">导入文件</button><button :disabled="!ready || busy || demo" @click="newWord">新增词条</button><button v-if="currentLibrary && !demo" class="danger-button" :disabled="busy" @click="deleteLibrary">删除词库</button></div></header>
      <p v-if="demo" class="demo-banner" role="status">浏览器演示 · 下方是 8 条隔离样例，不是正式词库或 Excel 导入结果。可查看、检索和切换词库；保存及文件操作请使用桌面版。</p>
      <p v-if="error" class="error" role="alert">{{ error }}</p><p v-if="notice" class="notice" role="status">{{ notice }}</p>
      <StoragePanel v-if="!ready" :storage="storage" />
      <section v-if="ready" class="formal-controls" aria-label="检索筛选">
        <label class="search"><span>检索</span><input ref="searchInput" v-model="query.text" :aria-label="demo ? '搜索演示词条' : '搜索真实词条'" placeholder="写法、假名、中文义项、备注" @input="schedule" @compositionstart="beginComposition" @compositionend="endComposition" /><kbd>Ctrl / ⌘ F</kbd></label>
        <div class="filters">
          <label>JLPT<select v-model="query.jlpt" @change="schedule"><option :value="null">全部</option><option v-for="level in ['N5','N4','N3','N2','N1']" :key="level">{{ level }}</option></select></label>
          <label>词性<select v-model="query.pos" @change="schedule"><option :value="null">全部</option><option v-for="[code, label] in partsOfSpeech" :key="code" :value="code">{{ label }}</option></select></label>
          <label>卡片状态<select v-model="query.state" @change="schedule"><option :value="null">全部</option><option v-for="state in ['new','learning','review','relearning']" :key="state">{{ state }}</option></select></label>
          <label>排序<select v-model="query.sort" @change="schedule"><option value="updated_at">更新时间</option><option value="kanji">写法</option><option value="kana">读音</option><option value="due_at">下次复习</option></select></label>
        </div>
        <details class="advanced-filters"><summary>更多筛选</summary><div class="filters">
          <label>到期<select v-model="query.due" @change="schedule"><option :value="null">全部</option><option :value="true">已到期</option><option :value="false">未到期或未安排</option></select></label>
          <label>方向<select v-model="query.descending" @change="schedule"><option :value="true">降序</option><option :value="false">升序</option></select></label>
        </div></details>
        <div class="filter-chips">
          <button v-if="query.text" class="text-button" @click="query.text = ''; schedule()">关键词：{{ query.text }} ×</button>
          <button v-if="query.jlpt" class="text-button" @click="query.jlpt = null; schedule()">{{ query.jlpt }} ×</button>
          <button v-if="query.pos" class="text-button" @click="query.pos = null; schedule()">{{ query.pos }} ×</button>
          <button v-if="query.state" class="text-button" @click="query.state = null; schedule()">状态：{{ query.state }} ×</button>
          <button v-if="query.due !== null" class="text-button" @click="query.due = null; schedule()">{{ query.due ? '已到期' : '未到期或未安排' }} ×</button>
          <button class="text-button" @click="query.text = ''; query.jlpt = null; query.pos = null; query.state = null; query.due = null; schedule()">清除筛选</button>
        </div>
      </section>
      <section v-if="selected.length && !demo" class="batch-toolbar" aria-label="批量操作">
        <strong>当前已选 {{ selected.length }} 条</strong>
        <button :disabled="busy" @click="emit('data', selected.map(word => word.id), 'export')">导出所选词条</button>
        <select v-model="batchTarget" aria-label="目标词库"><option value="">选择词库</option><option v-for="library in libraries" :key="library.id" :value="library.id">{{ library.name }}</option></select>
        <button :disabled="busy || !batchTarget" @click="batch('add_library', batchTarget)">加入词库</button><button :disabled="busy || !batchTarget" @click="batch('remove_library', batchTarget)">移出词库</button>
        <button class="danger-button" :disabled="busy" @click="deleteWords">永久删除词条…</button><button class="text-button" @click="selected = []">清空选择</button>
      </section>
      <section v-if="ready" class="formal-workspace">
        <div class="list-panel"><div class="panel-heading"><h2>{{ total }} 个匹配词条</h2><button v-if="detail && !showDetail" class="text-button" @click="showDetail = true">查看当前词条</button></div>
          <WordList :words="words" :active-id="activeId" :selected="selected" :loading="loading" :readonly="demo" @select="openWord" @toggle="toggle" @range="range" @open="showDetail = true" />
          <footer><span>{{ loading ? '正在更新…' : `已加载 ${words.length} / ${total} 条` }}</span><button v-if="cursor" :disabled="loading" @click="search(true)">加载下一页</button></footer>
        </div>
      </section>
    </main>
    <Transition name="drawer" @after-leave="detailExited"><WordDetailView v-if="showDetail && (detail || detailLoading)" :word="detailLoading ? undefined : detail" :libraries="libraries" :readonly="demo" @edit="editWord" @delete="deleteCurrentWord" @close="closeDetail" /></Transition>
    <Transition name="dialog"><LibraryEditor v-if="libraryDialog" ref="libraryEditor" :library="editingLibrary" @close="closeEditor" @saved="savedLibrary" @continue-editing="exitRequested = false" /></Transition>
    <Transition name="dialog"><WordEditor v-if="wordDialog" ref="editor" :word="editingWord" :libraries="libraries" :library-id="query.library_id" @close="closeEditor" @saved="savedWord" @continue-editing="exitRequested = false" /></Transition>
    <Transition name="dialog"><EditorDialog v-if="confirmation" :title="confirmation.title" :busy="busy" @close="closeConfirmation"><div class="editor-form"><p v-if="error" class="error" role="alert">{{ error }}</p><p>{{ confirmation.message }}</p><p v-if="confirmation.stage === 2" class="error">第二次确认：此操作将永久删除上述数据。</p><button class="danger-button" :disabled="busy" @click="confirm">{{ confirmation.second && confirmation.stage === 1 ? '我已了解，继续确认' : '确认执行' }}</button><button :disabled="busy" @click="confirmation = undefined">取消</button></div></EditorDialog></Transition>
  </div>
</template>
