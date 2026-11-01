<script setup lang="ts">
import type { StorageStatus } from '../api';

defineProps<{ storage?: StorageStatus }>();
</script>

<template>
  <section class="storage-panel" aria-label="正式数据底座">
    <div class="panel-heading">
      <h2>正式数据底座 · M1-A</h2>
      <span class="badge">
        {{ storage?.state === 'ready' ? '存储已就绪' : storage?.state === 'blocked' ? '存储受阻' : '正在检查' }}
      </span>
    </div>
    <div v-if="storage?.state === 'ready'" class="storage-content">
      <p>Schema {{ storage.schema_version }} · {{ storage.library_count }} 个词库 · {{ storage.word_count }} 个真实词条</p>
      <p class="muted">正式库与合成验证数据隔离。返回真实词库可管理词库和词条。</p>
    </div>
    <div v-if="storage?.error" class="error" role="alert">
      {{ storage.error.code }}：{{ storage.error.message }}。请保留原数据，不要删除数据库。
    </div>
    <p v-if="storage" class="storage-path">{{ storage.data_directory }}</p>
  </section>
</template>
