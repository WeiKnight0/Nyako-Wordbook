<script setup lang="ts">
import { computed } from 'vue';
import { partsOfSpeech } from '../vocabulary';

const props = defineProps<{ incoming: Record<string, unknown>; existing?: Record<string, unknown>; present: string[] }>();
const labels: Record<string, string> = { kanji: '写法', kana: '假名', parts_of_speech: '词性', jlpt: 'JLPT', pitch: '音调', meaning: '释义', examples_jp: '日文例句', examples_cn: '例句中文翻译', note: '备注' };
const fields = computed(() => Object.keys(labels).filter(field => props.present.includes(field)));
function display(field: string, value: unknown): string {
  if (value == null || value === '' || Array.isArray(value) && !value.length) return '（空）';
  if (!Array.isArray(value)) return String(value);
  if (field === 'parts_of_speech') return value.map(code => partsOfSpeech.find(([id]) => id === code)?.[1] ?? code).join(' / ');
  return value.join(' / ');
}
</script>

<template>
  <div class="comparison-scroll">
    <table class="import-comparison" :aria-label="existing ? '导入字段对比' : '导入词条内容'">
      <thead><tr><th scope="col">字段</th><th v-if="existing" scope="col">当前词条</th><th scope="col">文件内容</th></tr></thead>
      <tbody><tr v-for="field in fields" :key="field" :class="{ changed: existing && JSON.stringify(existing[field]) !== JSON.stringify(incoming[field]) }">
        <th scope="row">{{ labels[field] }}<span v-if="existing && JSON.stringify(existing[field]) !== JSON.stringify(incoming[field])" class="difference">有差异</span></th>
        <td v-if="existing" :lang="['kanji', 'kana'].includes(field) ? 'ja' : undefined">{{ display(field, existing[field]) }}</td>
        <td :lang="['kanji', 'kana'].includes(field) ? 'ja' : undefined">{{ display(field, incoming[field]) }}</td>
      </tr></tbody>
    </table>
  </div>
  <p class="comparison-help">仅列出文件提供的字段；最终写入由所选更新策略决定。</p>
</template>

<style scoped>
.comparison-scroll { max-height: 320px; overflow: auto; margin-top: 10px; }
.import-comparison { width: 100%; table-layout: fixed; border-collapse: collapse; font-size: 13px; }
th, td { padding: 10px; border: 1px solid #dce5dd; text-align: left; vertical-align: top; white-space: pre-wrap; overflow-wrap: anywhere; }
thead th { position: sticky; top: 0; background: #edf3ed; }
th:first-child { width: 100px; }
.changed { background: #fff7e5; }
.difference { display: block; margin-top: 4px; color: #87540f; font-size: 11px; font-weight: normal; }
.comparison-help { color: #687b72; font-size: 12px; }
</style>
