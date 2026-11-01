import { ref } from 'vue';
import { api, type Diagnostics, type Health, type Rating, type StorageStatus } from '../api';

/**
 * @description 顺序加载正式存储与合成数据验证结果。
 * @param 无。
 * @throws 加载函数向事件入口传播原生后端错误。
 */
export function useDiagnostics() {
  const storage = ref<StorageStatus>();
  const health = ref<Health>();
  const diagnostics = ref<Diagnostics>();
  const ratings = ref<Rating[]>([]);
  const verifying = ref(false);

  async function verify() {
    verifying.value = true;
    try {
      storage.value = await api.storage();
      diagnostics.value = await api.diagnostics();
      ratings.value = await api.scheduler();
    } finally {
      verifying.value = false;
    }
  }

  async function initialize() {
    health.value = await api.health();
    await verify();
  }

  return { storage, health, diagnostics, ratings, verifying, verify, initialize };
}
