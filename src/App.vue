<script setup lang="ts">
import { ref } from 'vue';
import LibraryView from './views/LibraryView.vue';
import DiagnosticsView from './views/DiagnosticsView.vue';
import DataView from './views/DataView.vue';

const diagnostics = ref(false);
const data = ref(false);
const exportSelection = ref<string[]>([]);
const dataSection = ref<'import' | 'export' | 'backup'>('import');
const dataEntry = ref(0);
function openData(selected: string[] = [], section: 'import' | 'export' | 'backup' = 'import') {
  exportSelection.value = selected;
  dataSection.value = section;
  dataEntry.value++;
  data.value = true;
}
</script>

<template>
  <Transition name="page"><KeepAlive><DataView v-if="data" :selected="exportSelection" :entry-section="dataSection" :entry="dataEntry" @back="data = false" /></KeepAlive></Transition>
  <Transition name="page"><LibraryView v-if="!data && !diagnostics" @diagnostics="diagnostics = true" @data="openData" /></Transition>
  <Transition name="page"><div v-if="!data && diagnostics" class="diagnostics-page">
    <button class="return-library" @click="diagnostics = false">返回真实词库</button>
    <DiagnosticsView />
  </div></Transition>
</template>
