<script setup lang="ts">
import { ref } from 'vue';
import LibraryView from './views/LibraryView.vue';
import DiagnosticsView from './views/DiagnosticsView.vue';
import DataView from './views/DataView.vue';

const diagnostics = ref(false);
const data = ref(false);
const exportSelection = ref<string[]>([]);
function openData(selected: string[] = []) { exportSelection.value = selected; data.value = true; }
</script>

<template>
  <KeepAlive><DataView v-if="data" :selected="exportSelection" @back="data = false" /></KeepAlive>
  <LibraryView v-if="!data && !diagnostics" @diagnostics="diagnostics = true" @data="openData" />
  <template v-if="!data && diagnostics">
    <button class="return-library" @click="diagnostics = false">返回真实词库</button>
    <DiagnosticsView />
  </template>
</template>
