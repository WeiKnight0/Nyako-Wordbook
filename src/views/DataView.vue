<script setup lang="ts">
import { onActivated, onDeactivated, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { desktop, errorMessage } from '../api';
import { vocabulary, type Library } from '../vocabulary';
import { demoLibraries, demoWords } from '../demoVocabulary';

interface Preview { task_id: string; valid: number; errors: number; total: number; format: string; columns: string[] }
const classifications: Record<string, string> = { new: '新建', same: '内容相同', update: '可更新', ambiguous: '身份歧义', error: '错误' };
interface Row { classification: string; locally_modified: boolean; line: number; content: Record<string, unknown> | null; error: string | null; candidates: { id: string; kana: string; meaning: string; content: Record<string, unknown> }[]; matched: string | null }
interface Choice { line: number; action: string; target: string | null; strategy: string; fields: string[]; clear: string[] }
const props = withDefaults(defineProps<{ selected?: string[] }>(), { selected: () => [] });
const selectionExport = ref(props.selected.length > 0);
const section = ref<'import' | 'export' | 'backup'>(props.selected.length ? 'export' : 'import');
watch(() => props.selected, selected => {
  selectionExport.value = selected.length > 0;
  if (selected.length) section.value = 'export';
});
let active = true;
onActivated(() => { active = true; });
onDeactivated(() => { active = false; });
const emit = defineEmits<{ back: [] }>();
const demo = import.meta.env.DEV && !desktop;
const libraries = ref<Library[]>([]);
const busy = ref(false);
const error = ref('');
const notice = ref('');
const path = ref('');
const namespace = ref('nyako:excel:local');
watch(path, () => { sample.value = undefined; preview.value = undefined; });
const sample = ref<{ sheets: string[]; sheet: string; columns: string[]; samples: string[][] }>();
async function inspectExcel() {
  await action(async () => {
    sample.value = await exchange({ operation: 'inspect_excel', path: path.value, options: { sheet: null, headers: true, columns: {}, pos_mapping: {} } });
  });
}
const library = ref('');
const newLibrary = ref('');
const preview = ref<Preview>();
const pendingImports = ref<Preview[]>([]);
async function findPending() { await action(async () => { pendingImports.value = await exchange<Preview[]>({ operation: 'pending_imports' }); }); }
async function resumeImport(task: string) {
  await action(async () => {
    preview.value = await exchange<Preview>({ operation: 'resume_import', task_id: task });
    choices.value = {}; offset.value = 0; errorsOnly.value = false; validOnly.value = false; await page();
    notice.value = '已重新核对源文件和当前词库，请重新选择冲突策略与目标词库。';
  });
}
const rows = ref<Row[]>([]);
const choices = ref<Record<number, Choice>>({});
const offset = ref(0);
const errorsOnly = ref(false);
const validOnly = ref(false);
const restore = ref<{ task_id: string; manifest: Record<string, unknown> }>();
const restoreConfirmed = ref(false);
const restartRequired = ref(false);
let unlisten: (() => void) | undefined;
let unlistenClose: (() => void) | undefined;
const fields = ['kanji', 'kana', 'parts_of_speech', 'jlpt', 'pitch_accents', 'senses', 'examples', 'tags', 'note'];
const job = ref<{ id: string; processed: number; cancellable: boolean }>();
async function exchange<T>(request: object): Promise<T> {
  const started = await invoke<{ job_id: string }>('exchange', { request });
  job.value = { id: started.job_id, processed: 0, cancellable: true };
  try {
    while (true) {
      const status = await invoke<{ id: string; state: string; processed: number; cancellable: boolean; result: T; error: unknown }>('exchange_status', { id: started.job_id, cancel: false });
      job.value = status;
      if (status.state !== 'running') {
        if (status.error) throw status.error;
        return status.result;
      }
      await new Promise(resolve => setTimeout(resolve, 200));
    }
  } finally { job.value = undefined; }
}
async function cancelJob() {
  if (!job.value) return;
  try { await invoke('exchange_status', { id: job.value.id, cancel: true }); }
  catch (cause) { error.value = errorMessage(cause); }
}
async function action(run: () => Promise<void>) {
  if (busy.value || restartRequired.value) return;
  busy.value = true; error.value = ''; notice.value = '';
  try { await run(); } catch (cause) { error.value = errorMessage(cause); }
  finally { busy.value = false; }
}
async function choose(save: boolean, format: string) {
  return invoke<string | null>('exchange_file', { save, format });
}
async function selectImport() {
  await action(async () => { const selected = await choose(false, 'import'); if (selected) { path.value = selected; preview.value = undefined; sample.value = undefined; } });
}
async function saveTemplate() {
  await action(async () => {
    const output = await choose(true, 'xlsx'); if (!output) return;
    const expected = await invoke('exchange_target', { path: output });
    if (expected && !window.confirm('确认替换已有 Excel 模板？')) return;
    await exchange({ operation: 'excel_template', path: output, expected });
    notice.value = '单表 Excel 模板已保存，编辑词条工作表后即可导入。';
  });
}
async function page() {
  if (!preview.value) return;
  rows.value = await exchange<Row[]>({ operation: 'page', task_id: preview.value.task_id, offset: offset.value, errors: errorsOnly.value });
  rows.value.filter(row => row.content).forEach(row => {
    choices.value[row.line] ??= { line: row.line, action: row.matched ? 'reuse' : row.candidates.length ? 'unresolved' : 'new', target: row.matched, strategy: 'keep', fields: [], clear: [] };
  });
}
function batchChoices(classification: string, strategy: string) {
  rows.value.filter(row => row.classification === classification && row.content).forEach(row => {
    const choice = choices.value[row.line]!;
    if (strategy === 'skip') { choice.action = 'skip'; return; }
    if (!row.matched && row.candidates.length !== 1) return;
    choice.action = 'reuse'; choice.target = row.matched ?? row.candidates[0]!.id; choice.strategy = strategy;
  });
}
async function parse() {
  await action(async () => {
    preview.value = await exchange<Preview>({ operation: 'preview', path: path.value, options: { source_namespace: namespace.value, excel: { sheet: null, headers: true, columns: {}, pos_mapping: {} } } });
    choices.value = {}; offset.value = 0; await page();
  });
}
async function commit() {
  if (!preview.value) return;
  const selected = Object.values(choices.value);
  if (selected.some(choice => choice.action === 'unresolved')) { error.value = '当前已查看的行仍有未解决身份冲突。'; return; }
  await action(async () => {
    const result = await exchange<{ created: number; updated: number; unchanged: number; skipped: number }>({ operation: 'commit', task_id: preview.value!.task_id, options: { library_id: library.value || null, new_library: newLibrary.value || null, valid_only: validOnly.value, choices: Object.values(choices.value) } });
    notice.value = `导入完成：新增 ${result.created} 条，更新 ${result.updated} 条，未变化 ${result.unchanged} 条，跳过 ${result.skipped} 条。返回词库即可查看。`;
    preview.value = undefined; rows.value = [];
    libraries.value = await vocabulary<Library[]>({ operation: 'libraries' });
  });
}
async function exportFile(format: string) {
  await action(async () => {
    const output = await choose(true, format); if (!output) return;
    const expected = await invoke('exchange_target', { path: output });
    if (expected && !window.confirm('目标文件已存在，确认完整替换？失败时保留原文件。')) return;
    const result = await exchange({ operation: 'export', options: { path: output, format, library_id: selectionExport.value ? null : library.value || null, selected: selectionExport.value ? props.selected : [], expected } });
    notice.value = `导出完成：${JSON.stringify(result)}`;
  });
}
async function backup() {
  await action(async () => {
    const output = await choose(true, 'nyakobak'); if (!output) return;
    const expected = await invoke('exchange_target', { path: output });
    if (expected && !window.confirm('目标备份已存在，确认完整替换？')) return;
    await exchange({ operation: 'backup', path: output, expected }); notice.value = '完整备份已生成并通过校验。';
  });
}
async function errorReport() {
  if (!preview.value) return;
  await action(async () => {
    const output = await choose(true, 'json'); if (!output) return;
    const expected = await invoke('exchange_target', { path: output });
    if (expected && !window.confirm('确认替换已有错误报告？')) return;
    await exchange({ operation: 'error_report', task_id: preview.value!.task_id, path: output, expected });
    notice.value = '错误报告已保存为 JSON 数组，每项包含行号、字段与原因。';
  });
}
async function inspectBackup() {
  await action(async () => { const source = await choose(false, 'nyakobak'); if (!source) return; restore.value = await exchange({ operation: 'validate_backup', path: source }); restoreConfirmed.value = false; });
}
async function restoreBackup() {
  if (!restore.value || !restoreConfirmed.value) return;
  await action(async () => {
    await exchange({ operation: 'restore', task_id: restore.value!.task_id });
    notice.value = '当前数据的安全备份已保存。请关闭并重新启动应用，启动时将恢复整库。';
    restartRequired.value = true;
  });
}
onMounted(async () => {
  if (demo) { libraries.value = demoLibraries; return; }
  if (!desktop) return;
  await action(async () => { libraries.value = await vocabulary<Library[]>({ operation: 'libraries' }); });
  try {
    unlisten = await getCurrentWindow().onDragDropEvent(event => {
      if (event.payload.type !== 'drop' || busy.value || !active) return;
      path.value = event.payload.paths[0] ?? ''; preview.value = undefined;
    });
    unlistenClose = await getCurrentWindow().onCloseRequested(event => { if (busy.value) { event.preventDefault(); error.value = '文件任务正在执行，请完成后关闭。'; } });
  } catch (cause) { error.value = errorMessage(cause); }
});
onBeforeUnmount(() => { unlisten?.(); unlistenClose?.(); });
</script>

<template>
  <main class="data-view">
    <header class="data-header"><button class="data-back" @click="emit('back')">← 返回词库</button><div><div class="eyebrow">词汇管理 / 文件</div><h1>数据管理</h1></div></header>
    <nav class="data-tabs" aria-label="数据管理任务"><button :class="{ active: section === 'import' }" :aria-current="section === 'import' ? 'page' : undefined" @click="section = 'import'">导入</button><button :class="{ active: section === 'export' }" :aria-current="section === 'export' ? 'page' : undefined" @click="section = 'export'">导出</button><button :class="{ active: section === 'backup' }" :aria-current="section === 'backup' ? 'page' : undefined" @click="section = 'backup'">备份与恢复</button></nav>
    <div class="data-body">
    <p v-if="demo" class="demo-banner" role="status">浏览器演示 · 此页只展示操作布局和隔离样例，不读取本机 Excel，也不保存、导出或恢复正式数据。实际导入请使用 npm run desktop:dev。</p>
    <p v-if="error" class="error" role="alert">{{ error }}</p><p v-if="notice" class="notice" role="status">{{ notice }}</p>
    <div v-if="busy" class="data-progress" role="status">正在处理文件 <span v-if="job">· 已处理 {{ job.processed }} 项</span><button v-if="job?.cancellable" class="secondary-button" @click="cancelJob">取消当前任务</button></div>
    <section v-show="section === 'import'" class="data-section" aria-label="导入词条">
      <div class="section-heading"><div><h2>导入词条</h2><p>支持 Excel 和 JSON，先校验内容，再选择保存位置。</p></div><button class="text-button" :disabled="demo" @click="saveTemplate">下载 Excel 模板</button></div>
    <div v-if="demo" class="data-panel demo-preview"><div class="preview-count"><strong>样例工作簿 · {{ demoWords.length }} 条词条</strong><span>仅预览布局，不代表已校验或导入任何文件</span></div><div class="sample-table"><table><thead><tr><th>词条标识</th><th>写法</th><th>假名</th><th>释义</th><th>词性</th><th>音调</th><th>JLPT</th><th>日文例句</th><th>例句中文翻译</th><th>备注</th></tr></thead><tbody><tr v-for="word in demoWords" :key="word.id"><td>{{ word.id }}</td><td lang="ja">{{ word.kanji }}</td><td lang="ja">{{ word.kana }}</td><td>{{ word.meaning }}</td><td>{{ word.pos.join('|') }}</td><td></td><td>{{ word.jlpt }}</td><td lang="ja"></td><td></td><td></td></tr></tbody></table></div></div>
    <fieldset :disabled="busy || restartRequired || demo" class="data-panel">
      <legend>1 · 选择文件</legend>
      <div class="file-picker"><button @click="selectImport">选择文件</button><label>文件路径（也可拖入） <input v-model="path" /></label></div>
      <label>来源标识（用于再次导入时识别同一批数据） <input v-model="namespace" placeholder="例如 my-dictionary" /></label>
      <button class="secondary-button" :disabled="!path" @click="inspectExcel">查看样本</button>
      <div v-if="sample" class="sample-table"><table><thead><tr><th v-for="(name, index) in sample.columns" :key="index">{{ name }}</th></tr></thead><tbody><tr v-for="(record, index) in sample.samples" :key="index"><td v-for="(cell, column) in record" :key="column" :lang="[1, 2, 7].includes(column) ? 'ja' : 'zh-CN'">{{ cell }}</td></tr></tbody></table></div>
    </fieldset>
    <fieldset :disabled="busy || restartRequired || demo" class="data-panel">
      <legend>2 · 校验内容</legend><button :disabled="!path" @click="parse">全量校验并预览</button>
      <p v-if="preview" class="preview-count">共 {{ preview.total }} 行 <strong>{{ preview.valid }} 条有效</strong> <strong :class="{ 'has-errors': preview.errors }">{{ preview.errors }} 条错误</strong></p>
      <template v-if="preview">
        <div class="preview-tools"><span>当前页冲突处理</span><button class="secondary-button" @click="batchChoices('same', 'keep')">相同内容复用</button><button class="secondary-button" @click="batchChoices('update', 'keep')">可更新项保留已有</button><button class="secondary-button" @click="batchChoices('update', 'fill')">可更新项仅填空</button><button class="secondary-button" @click="batchChoices('ambiguous', 'skip')">歧义项跳过</button><label><input v-model="errorsOnly" type="checkbox" @change="offset = 0; action(page)" />只看错误</label><button v-if="preview.errors" class="text-button" @click="errorReport">保存错误报告</button></div>
        <div class="preview-rows"><article v-for="row in rows" :key="row.line" class="preview-row">
          <div class="preview-row-heading"><strong>第 {{ row.line }} 行 · {{ classifications[row.classification] }}</strong><span v-if="row.content">{{ row.content.kanji || row.content.kana }} · {{ Array.isArray(row.content.senses) ? (row.content.senses[0] as { meaning: string } | undefined)?.meaning : '' }}</span></div><p v-if="row.locally_modified">已有内容与上次导入不同，请核对本地修改后选择覆盖字段。</p><p v-if="row.error" role="alert">{{ row.error }}</p>
          <details v-if="row.content"><summary>查看完整词条内容</summary><pre>{{ JSON.stringify(row.content, null, 2) }}</pre></details>
          <template v-if="choices[row.line]">
            <label>身份 <select v-model="choices[row.line]!.action"><option value="unresolved">请选择</option><option value="new" :disabled="Boolean(row.matched)">新建独立词条</option><option value="reuse">复用候选</option><option value="skip">跳过</option></select></label>
            <select v-if="choices[row.line]!.action === 'reuse'" v-model="choices[row.line]!.target"><option :value="null">选择候选</option><option v-for="candidate in row.candidates" :key="candidate.id" :value="candidate.id">{{ candidate.kana }} · {{ candidate.meaning }}</option></select>
            <details v-for="candidate in row.candidates.filter(item => item.id === choices[row.line]!.target)" :key="candidate.id"><summary>查看已有完整内容</summary><pre>{{ JSON.stringify(candidate.content, null, 2) }}</pre></details>
            <label>更新策略 <select v-model="choices[row.line]!.strategy"><option value="keep">保留已有</option><option value="fill">仅填空</option><option value="replace">替换所选字段</option></select></label>
            <div v-if="choices[row.line]!.strategy === 'replace'"><label v-for="field in fields" :key="field"><input v-model="choices[row.line]!.fields" type="checkbox" :value="field" />{{ field }} <input v-model="choices[row.line]!.clear" type="checkbox" :value="field" />允许清空</label></div>
          </template>
        </article></div>
        <div class="preview-pagination"><button class="secondary-button" :disabled="offset === 0" @click="offset = Math.max(0, offset - 50); action(page)">上一页</button><span>第 {{ offset + 1 }}–{{ offset + rows.length }} 行</span><button class="secondary-button" :disabled="rows.length < 50" @click="offset += 50; action(page)">下一页</button></div>
      </template>
    </fieldset>
    <fieldset :disabled="busy || restartRequired || demo" class="data-panel">
      <legend>3 · 保存到词库</legend>
      <div class="destination"><label>现有词库 <select v-model="library"><option value="">不指定</option><option v-for="item in libraries" :key="item.id" :value="item.id">{{ item.name }}</option></select></label><span>或</span><label>新建词库 <input v-model="newLibrary" maxlength="100" placeholder="输入新词库名称" /></label></div>
      <div v-if="preview" class="commit-bar"><div><strong>准备导入 {{ preview.valid }} 条有效词条</strong><p>未查看行由后端继续检查；如有身份歧义，提交会被阻止。</p><label v-if="preview.errors"><input v-model="validOnly" type="checkbox" />我确认仅导入有效行</label></div><button :disabled="Boolean(preview.errors && !validOnly)" @click="commit">确认并原子提交</button></div>
      <p v-else class="muted">完成文件校验后，在这里确认导入目标。</p>
    </fieldset>
    <div class="pending-area"><button class="text-button" :disabled="busy || restartRequired || demo" @click="findPending">检查未完成导入</button><div v-if="pendingImports.length"><p>继续时将重新核对文件和词库，冲突选择不会自动恢复。</p><button v-for="task in pendingImports" :key="task.task_id" :disabled="busy" @click="resumeImport(task.task_id)">{{ task.task_id }} · {{ task.valid }} 条有效 · 重新核对</button></div></div>
    </section>
    <section v-show="section === 'export'" class="data-section" aria-label="导出词条"><div class="section-heading"><div><h2>导出词条</h2><p>Excel 固定单表十列；多义项和例句逐行编号，多音调用斜杠连接。</p></div></div><fieldset :disabled="busy || restartRequired || demo" class="data-panel"><legend>导出范围</legend><label v-if="props.selected.length"><input v-model="selectionExport" type="checkbox" />仅导出选中的 {{ props.selected.length }} 条词条</label><label v-if="!selectionExport">词库 <select v-model="library"><option value="">请选择词库</option><option v-for="item in libraries" :key="item.id" :value="item.id">{{ item.name }}</option></select></label><div class="export-actions"><button :disabled="!library && !selectionExport" @click="exportFile('xlsx')">导出 Excel</button><button class="secondary-button" :disabled="!library && !selectionExport" @click="exportFile('json')">导出 JSON</button></div></fieldset></section>
    <section v-show="section === 'backup'" class="data-section" aria-label="备份与恢复"><div class="section-heading"><div><h2>备份与恢复</h2><p>备份完整新版数据库；恢复将覆盖当前新版数据。</p></div></div><fieldset :disabled="busy || restartRequired || demo" class="data-panel"><legend>完整备份</legend><button @click="backup">创建完整备份</button></fieldset><fieldset :disabled="busy || restartRequired || demo" class="data-panel"><legend>恢复数据</legend><button class="secondary-button" @click="inspectBackup">选择备份并完整校验</button><template v-if="restore"><pre>{{ JSON.stringify(restore.manifest, null, 2) }}</pre><label><input v-model="restoreConfirmed" type="checkbox" />我确认覆盖当前新版数据库中的词库和学习状态</label><button class="danger-button" :disabled="!restoreConfirmed" @click="restoreBackup">安全备份后安排恢复</button></template></fieldset></section>
    </div>
  </main>
</template>

<style scoped>
.data-view { width: 100%; max-width: none; min-height: 100vh; margin: 0; padding: 0; overflow: visible; }
.data-header { display: flex; flex-direction: row; align-items: center; justify-content: flex-start; gap: 24px; margin: 0; padding: 24px clamp(20px, 4vw, 56px); border-bottom: 1px solid #dce4de; background: #fff; }
.data-header h1 { margin: 3px 0 0; }
.data-back { padding: 9px 12px; border: 1px solid #cddbd3; background: #fff; color: #255447; white-space: nowrap; }
.data-tabs { display: flex; gap: 26px; padding: 0 clamp(20px, 4vw, 56px); border-bottom: 1px solid #dce4de; background: #fff; }
.data-tabs button { border: 0; border-radius: 0; padding: 16px 4px 13px; background: none; color: #647a70; font-weight: 600; white-space: nowrap; }
.data-tabs button.active { color: #215e4a; box-shadow: inset 0 -3px #278262; }
.data-body { width: min(100%, 1480px); margin: 0 auto; padding: 28px clamp(20px, 4vw, 56px) 64px; }
.data-section { min-width: 0; }
.section-heading { display: flex; align-items: start; justify-content: space-between; flex-wrap: wrap; gap: 16px; margin-bottom: 20px; }
.section-heading h2 { font-size: 21px; }
.section-heading p, .commit-bar p { margin: 6px 0 0; color: #687b72; font-size: 12px; }
.data-panel { min-width: 0; margin: 0 0 18px; padding: 20px 24px 24px; border: 1px solid #d8e2d9; border-radius: 6px; background: #fff; }
.data-panel legend { padding: 0 8px; font-weight: 700; color: #31594e; }
.data-panel label { display: inline-flex; align-items: center; flex-wrap: wrap; gap: 8px; margin: 6px 12px 6px 0; }
.data-panel input:not([type=checkbox]), .data-panel textarea, .data-panel select { min-width: 0; max-width: 100%; padding: 9px 10px; border: 1px solid #cbd9cf; border-radius: 5px; background: #fff; color: #263b34; }
.data-panel input[type=checkbox] { accent-color: #277452; }
.file-picker { display: flex; align-items: end; flex-wrap: wrap; gap: 12px; }
.file-picker label { flex: 1 1 360px; display: grid; gap: 5px; }
.file-picker input { width: 100%; }
.import-options, .data-panel > details { margin: 12px 0; }
.import-options summary, .data-panel summary { cursor: pointer; color: #31594e; }
.sample-table { max-width: 100%; max-height: 320px; margin-top: 12px; overflow: auto; border: 1px solid #e2e9e1; }
.demo-preview .sample-table { max-height: 180px; }
.sample-table table { border-collapse: collapse; min-width: 100%; white-space: nowrap; }
.sample-table th, .sample-table td { padding: 9px 12px; border-bottom: 1px solid #edf1ea; text-align: left; white-space: pre-line; vertical-align: top; }
.sample-table th { position: sticky; top: 0; background: #edf3ed; }
.preview-count { display: flex; flex-wrap: wrap; gap: 14px; align-items: baseline; }
.preview-count strong { color: #267052; }
.preview-count .has-errors { color: #a24029; }
.preview-tools, .preview-pagination { display: flex; align-items: center; flex-wrap: wrap; gap: 9px; margin: 14px 0; }
.preview-tools > span { color: #65786e; font-size: 12px; }
.preview-rows { max-height: 48vh; min-height: 180px; overflow: auto; border-block: 1px solid #e0e8e0; }
.preview-row { padding: 13px 3px; border-bottom: 1px solid #edf1ea; }
.preview-row-heading { display: flex; flex-wrap: wrap; gap: 8px 18px; }
.preview-row-heading span { color: #576e63; }
.preview-row pre, .data-panel pre { max-height: 250px; overflow: auto; white-space: pre-wrap; overflow-wrap: anywhere; }
.destination { display: flex; align-items: center; flex-wrap: wrap; gap: 10px; }
.destination label { flex: 1 1 240px; display: grid; align-items: start; }
.destination select, .destination input { width: 100%; }
.commit-bar { display: flex; align-items: center; justify-content: space-between; gap: 20px; margin-top: 18px; padding-top: 18px; border-top: 1px solid #e4ebe5; }
.commit-bar button { flex-shrink: 0; }
.pending-area, .export-actions { margin-top: 12px; }
.data-progress { display: flex; align-items: center; gap: 16px; padding: 10px 14px; background: #e8f0e3; }
.secondary-button { border: 1px solid #b9cec1; background: #fff; color: #31594e; }
[role=alert] { color: #a0342c; }
@media (max-width: 650px) {
  .data-header { gap: 12px; padding-block: 16px; }
  .data-header h1 { font-size: 20px; }
  .data-tabs { gap: 14px; overflow-x: auto; }
  .data-body { padding-top: 20px; }
  .data-panel { padding: 14px; }
  .commit-bar { align-items: stretch; flex-direction: column; }
}
</style>
