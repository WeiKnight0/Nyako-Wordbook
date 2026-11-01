<script setup lang="ts">
import { onActivated, onDeactivated, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { desktop, errorMessage } from '../api';
import { vocabulary, type Library } from '../vocabulary';

interface Preview { task_id: string; valid: number; errors: number; total: number; format: string; columns: string[] }
const classifications: Record<string, string> = { new: '新建', same: '内容相同', update: '可更新', ambiguous: '身份歧义', error: '错误' };
interface Row { classification: string; locally_modified: boolean; line: number; content: Record<string, unknown> | null; error: string | null; candidates: { id: string; kana: string; meaning: string; content: Record<string, unknown> }[]; matched: string | null }
interface Choice { line: number; action: string; target: string | null; strategy: string; fields: string[]; clear: string[] }
const props = withDefaults(defineProps<{ selected?: string[] }>(), { selected: () => [] });
const selectionExport = ref(props.selected.length > 0);
watch(() => props.selected, selected => { selectionExport.value = selected.length > 0; });
let active = true;
onActivated(() => { active = true; });
onDeactivated(() => { active = false; });
const emit = defineEmits<{ back: [] }>();
const libraries = ref<Library[]>([]);
const busy = ref(false);
const error = ref('');
const notice = ref('');
const path = ref('');
const namespace = ref('');
const encoding = ref('utf-8');
const delimiter = ref('');
const headers = ref(true);
const mapping = ref('');
const posMapping = ref('');
const sample = ref<{ columns: string[]; samples: string[][] }>();
const columnMap = ref<Record<string, number | ''>>({});
const csvFields = ['external_id', 'kanji', 'kana', 'meaning', 'part_of_speech', 'pitch_accent', 'mora_count', 'jlpt', 'example_jp', 'example_cn', 'tags', 'note'];
async function inspectCsv() {
  await action(async () => {
    sample.value = await exchange({ operation: 'inspect_csv', path: path.value, options: { encoding: encoding.value, delimiter: delimiter.value || null, headers: headers.value, columns: {}, pos_mapping: {} } });
    columnMap.value = Object.fromEntries(csvFields.map(field => [field, sample.value!.columns.includes(field) ? sample.value!.columns.indexOf(field) : '']));
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
const csvLoss = ref(false);
const csvOriginal = ref(false);
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
  await action(async () => { const selected = await choose(false, 'import'); if (selected) { path.value = selected; preview.value = undefined; } });
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
    const columns = mapping.value.trim() ? JSON.parse(mapping.value) : Object.fromEntries(Object.entries(columnMap.value).filter(([, index]) => index !== ''));
    const pos_mapping = posMapping.value.trim() ? JSON.parse(posMapping.value) : {};
    preview.value = await exchange<Preview>({ operation: 'preview', path: path.value, options: { source_namespace: namespace.value, csv: { encoding: encoding.value, delimiter: delimiter.value || null, headers: headers.value, columns, pos_mapping } } });
    choices.value = {}; offset.value = 0; await page();
  });
}
async function commit() {
  if (!preview.value) return;
  const selected = Object.values(choices.value);
  if (selected.some(choice => choice.action === 'unresolved')) { error.value = '当前已查看的行仍有未解决身份冲突。'; return; }
  await action(async () => {
    const result = await exchange({ operation: 'commit', task_id: preview.value!.task_id, options: { library_id: library.value || null, new_library: newLibrary.value || null, valid_only: validOnly.value, choices: Object.values(choices.value) } });
    notice.value = `导入完成：${JSON.stringify(result)}`;
    preview.value = undefined; rows.value = [];
    libraries.value = await vocabulary<Library[]>({ operation: 'libraries' });
  });
}
async function exportFile(format: string) {
  await action(async () => {
    const output = await choose(true, format); if (!output) return;
    const expected = await invoke('exchange_target', { path: output });
    if (expected && !window.confirm('目标文件已存在，确认完整替换？失败时保留原文件。')) return;
    const result = await exchange({ operation: 'export', options: { path: output, format, library_id: selectionExport.value ? null : library.value || null, selected: selectionExport.value ? props.selected : [], csv_loss_confirmed: csvLoss.value, csv_original: csvOriginal.value, expected } });
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
  await action(async () => { libraries.value = await vocabulary<Library[]>({ operation: 'libraries' }); });
  if (!desktop) return;
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
    <header><button @click="emit('back')">返回词库</button><h1>数据管理</h1></header>
    <p v-if="error" role="alert">{{ error }}</p><p v-if="notice" role="status">{{ notice }}</p>
    <p v-if="busy" role="status">正在处理文件，请稍候…… <span v-if="job">已处理 {{ job.processed }} 项</span></p>
    <button v-if="job?.cancellable" @click="cancelJob">取消当前任务</button>
    <button :disabled="busy || restartRequired" @click="findPending">检查未完成导入</button>
    <div v-if="pendingImports.length"><p>发现已完成校验但未提交的导入。继续时重新核对源文件和目标，之前的冲突选择不自动恢复。</p><button v-for="task in pendingImports" :key="task.task_id" :disabled="busy" @click="resumeImport(task.task_id)">{{ task.task_id }} · {{ task.valid }} 条有效 · 重新核对</button></div>
    <fieldset :disabled="busy || restartRequired">
      <legend>导入与导出的目标词库</legend>
      <label>现有词库 <select v-model="library"><option value="">请选择</option><option v-for="item in libraries" :key="item.id" :value="item.id">{{ item.name }}</option></select></label>
      <label>或新建词库（仅导入） <input v-model="newLibrary" maxlength="100" /></label>
    </fieldset>
    <fieldset :disabled="busy || restartRequired">
      <legend>导入 CSV / JSON</legend>
      <button @click="selectImport">选择文件</button><label>文件（也可拖入） <input v-model="path" /></label>
      <label>CSV 来源命名空间 <input v-model="namespace" placeholder="例如 my-dictionary" /></label>
      <label>CSV 编码 <select v-model="encoding"><option value="utf-8">UTF-8</option><option value="cp932">CP932 / Shift_JIS</option></select></label>
      <label>分隔符 <select v-model="delimiter"><option value="">自动</option><option value=",">逗号</option><option :value="'\t'">制表符</option><option value=";">分号</option></select></label>
      <label><input v-model="headers" type="checkbox" />首行为表头</label>
      <button :disabled="!path" @click="inspectCsv">查看 CSV 样本与列映射</button>
      <template v-if="sample">
        <div><label v-for="field in csvFields" :key="field">{{ field }}<select v-model="columnMap[field]"><option value="">不导入</option><option v-for="(name, index) in sample.columns" :key="index" :value="index">第 {{ index + 1 }} 列 · {{ name }}</option></select></label></div>
        <div class="sample-table"><table><thead><tr><th v-for="(name, index) in sample.columns" :key="index">{{ name }}</th></tr></thead><tbody><tr v-for="(record, index) in sample.samples" :key="index"><td v-for="(cell, column) in record" :key="column">{{ cell }}</td></tr></tbody></table></div>
      </template>
      <details><summary>高级映射</summary><label>列映射（零基列号 JSON，覆盖下拉选择）<textarea v-model="mapping" placeholder='{"kana":0,"meaning":1}' /></label><label>自定义词性映射<textarea v-model="posMapping" placeholder='{"名詞":"noun"}' /></label></details>
      <button :disabled="!path" @click="parse">全量校验并预览</button>
      <template v-if="preview">
        <p>任务 {{ preview.task_id }}：共 {{ preview.total }} 行，有效 {{ preview.valid }}，错误 {{ preview.errors }}。未提交前词库不变。</p>
        <p>列：{{ preview.columns.join(' / ') }}</p>
        <p>批量操作仅作用于当前页。其他页的歧义仍需逐页确认。</p><button @click="batchChoices('same', 'keep')">本页相同内容复用</button><button @click="batchChoices('update', 'keep')">本页可更新项保留已有</button><button @click="batchChoices('update', 'fill')">本页可更新项仅填空</button><button @click="batchChoices('ambiguous', 'skip')">本页歧义全部跳过</button>
        <label><input v-model="errorsOnly" type="checkbox" @change="offset = 0; action(page)" />只看错误</label><button v-if="preview.errors" @click="errorReport">保存错误报告</button>
        <article v-for="row in rows" :key="row.line">
          <strong>第 {{ row.line }} 行 · {{ classifications[row.classification] }}</strong><p v-if="row.locally_modified">已有内容与上次导入不同，请核对本地修改后选择覆盖字段。</p><p v-if="row.error" role="alert">{{ row.error }}</p>
          <pre v-if="row.content">{{ JSON.stringify(row.content, null, 2) }}</pre>
          <template v-if="choices[row.line]">
            <label>身份 <select v-model="choices[row.line]!.action"><option value="unresolved">请选择</option><option value="new" :disabled="Boolean(row.matched)">新建独立词条</option><option value="reuse">复用候选</option><option value="skip">跳过</option></select></label>
            <select v-if="choices[row.line]!.action === 'reuse'" v-model="choices[row.line]!.target"><option :value="null">选择候选</option><option v-for="candidate in row.candidates" :key="candidate.id" :value="candidate.id">{{ candidate.kana }} · {{ candidate.meaning }}</option></select>
            <details v-for="candidate in row.candidates.filter(item => item.id === choices[row.line]!.target)" :key="candidate.id"><summary>查看已有完整内容，与上方导入内容比较</summary><pre>{{ JSON.stringify(candidate.content, null, 2) }}</pre></details>
            <label>更新策略 <select v-model="choices[row.line]!.strategy"><option value="keep">保留已有</option><option value="fill">仅填空</option><option value="replace">替换所选字段</option></select></label>
            <div v-if="choices[row.line]!.strategy === 'replace'"><label v-for="field in fields" :key="field"><input v-model="choices[row.line]!.fields" type="checkbox" :value="field" />{{ field }} <input v-model="choices[row.line]!.clear" type="checkbox" :value="field" />允许清空</label></div>
          </template>
        </article>
        <button :disabled="offset === 0" @click="offset = Math.max(0, offset - 50); action(page)">上一页</button><button :disabled="rows.length < 50" @click="offset += 50; action(page)">下一页</button>
        <p>已查看并记录选择 {{ Object.keys(choices).length }} 条：新建 {{ Object.values(choices).filter(choice => choice.action === 'new').length }}，复用 {{ Object.values(choices).filter(choice => choice.action === 'reuse').length }}，跳过 {{ Object.values(choices).filter(choice => choice.action === 'skip').length }}。未查看行仍需后端检查，歧义将阻止整批提交。</p>
        <label v-if="preview.errors"><input v-model="validOnly" type="checkbox" />我确认仅导入有效行</label>
        <button :disabled="Boolean(preview.errors && !validOnly)" @click="commit">确认并原子提交</button>
      </template>
    </fieldset>
    <fieldset :disabled="busy || restartRequired">
      <legend>导出所选词库</legend><p>JSON 保留完整业务内容。CSV 仅保留首义项、首例句和首音调，标签含 | 时请使用 JSON。</p>
      <label><input v-model="csvLoss" type="checkbox" />确认 CSV 有损导出</label><label><input v-model="csvOriginal" type="checkbox" />CSV 输出原值（关闭公式保护）</label>
      <label v-if="props.selected.length"><input v-model="selectionExport" type="checkbox" />仅导出从词库选中的 {{ props.selected.length }} 条</label>
      <button :disabled="!library && !selectionExport" @click="exportFile('json')">导出 JSON</button><button :disabled="(!library && !selectionExport) || !csvLoss" @click="exportFile('csv')">导出 CSV</button>
    </fieldset>
    <fieldset :disabled="busy || restartRequired">
      <legend>完整备份与恢复</legend><p>备份包含词库、学习状态及引用媒体。恢复覆盖全部数据，恢复前自动创建当前数据的安全备份。</p>
      <button @click="backup">创建完整备份</button><button @click="inspectBackup">选择备份并完整校验</button>
      <template v-if="restore"><pre>{{ JSON.stringify(restore.manifest, null, 2) }}</pre><label><input v-model="restoreConfirmed" type="checkbox" />我确认覆盖当前所有词库、学习状态和媒体</label><button :disabled="!restoreConfirmed" @click="restoreBackup">安全备份后安排恢复</button></template>
    </fieldset>
  </main>
</template>

<style scoped>
.data-view { max-width: 1100px; margin: auto; padding: 24px; overflow: auto; height: 100vh; box-sizing: border-box; }
header { display: flex; align-items: center; gap: 20px; }
fieldset { margin: 20px 0; padding: 20px; border: 1px solid #abb7bd; border-radius: 8px; }
label { display: inline-flex; align-items: center; gap: 8px; margin: 8px; flex-wrap: wrap; }
input:not([type=checkbox]), textarea { min-width: 220px; padding: 6px; }
button { margin: 6px; padding: 8px 14px; }
article { margin: 12px 0; border-top: 1px solid #abb7bd; padding-top: 12px; }
pre { white-space: pre-wrap; overflow-wrap: anywhere; max-height: 320px; overflow: auto; }
[role=alert] { color: #b93838; }
</style>
