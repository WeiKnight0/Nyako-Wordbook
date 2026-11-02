<script setup lang="ts">
import { computed, ref } from 'vue';
import { errorMessage } from '../api';
import { vocabulary, type Library } from '../vocabulary';
import EditorDialog from './EditorDialog.vue';

const props = defineProps<{ library?: Library }>();
const emit = defineEmits<{ saved: []; close: []; continueEditing: [] }>();
const original = props.library ? JSON.stringify(props.library) : '';
const form = ref<Library>(props.library ? { ...props.library } : { id: crypto.randomUUID(), name: '', description: '', archived: false, revision: 1, word_count: 0 });
const dirty = computed(() => props.library ? JSON.stringify(form.value) !== original : Boolean(form.value.name || form.value.description || form.value.archived));
const saving = ref(false);
const error = ref('');
const leaving = ref(false);
async function save(close = false) {
  if (saving.value) return;
  saving.value = true;
  error.value = '';
  try {
    await vocabulary<Library[]>({ operation: 'save_library', library: { ...form.value }, expected: props.library?.revision ?? null });
    if (close) leaving.value = false;
    emit('saved');
  } catch (cause) {
    error.value = errorMessage(cause);
    try {
      const current = (await vocabulary<Library[]>({ operation: 'libraries' })).find(item => item.id === form.value.id);
      error.value += current
        ? `；数据库已有此词库（版本 ${current.revision}）：${current.name}。请保留草稿，关闭后重开核对，避免重复创建。`
        : '；数据库未找到此词库，草稿已保留，可重试保存。';
    } catch (readError) { error.value += `；保存结果核对失败：${errorMessage(readError)}`; }
  }
  finally { saving.value = false; }
}
function requestClose() {
  if (saving.value) return;
  if (dirty.value) leaving.value = true;
  else emit('close');
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
  <EditorDialog :title="library ? '编辑词库' : '新建词库'" :busy="saving" @close="requestClose">
    <div v-if="leaving" class="dialog-decision"><p>词库有未保存修改，离开后草稿会丢失。</p><p v-if="error" class="error" role="alert">{{ error }}</p><div class="form-actions"><button :disabled="saving" @click="save(true)">保存并离开</button><button class="danger-button" :disabled="saving" @click="emit('close')">放弃修改</button><button class="secondary-button" :disabled="saving" @click="leaving = false; emit('continueEditing')">继续编辑</button></div></div>
    <form v-else class="editor-form" @submit.prevent="save()" @keydown="shortcut">
      <p v-if="error" class="error" role="alert">{{ error }}</p>
      <label>词库名称<input v-model="form.name" autofocus required maxlength="200" /></label>
      <label>描述<textarea v-model="form.description" rows="4" /></label>
      <label class="inline-label"><input v-model="form.archived" type="checkbox" />归档（保留内容）</label>
      <div class="form-actions"><button :disabled="saving">{{ saving ? '保存中…' : '保存词库' }}</button><button type="button" @click="requestClose">取消</button></div>
    </form>
  </EditorDialog>
</template>
