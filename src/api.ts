import { invoke, isTauri } from '@tauri-apps/api/core';

/**
 * @description 原生后端的版本与运行环境。
 * @param 不适用，数据接口。
 * @throws 不适用。
 */
export interface Health {
  app_version: string;
  sqlite_version: string;
  scheduler: string;
  platform: string;
  acceptance_measure?: boolean;
}

/**
 * @description 合成检索数据中的词条展示内容。
 * @param 不适用，数据接口。
 * @throws 不适用。
 */
export interface Word {
  id: number;
  kanji: string;
  kana: string;
  meaning: string;
}

/**
 * @description 检索结果与后续分页位置。
 * @param 不适用，数据接口。
 * @throws 不适用。
 */
export interface SearchPage {
  words: Word[];
  next_cursor: number | null;
  elapsed_ms: number;
}

/**
 * @description 初次学习反馈对应的 FSRS 状态预览。
 * @param 不适用，数据接口。
 * @throws 不适用。
 */
export interface Rating {
  rating: number;
  stability_days: number;
  difficulty: number;
  interval_days: number;
  due_at_ms: number;
}

/**
 * @description 合成数据库的可靠性验证结果。
 * @param 不适用，数据接口。
 * @throws 不适用。
 */
export interface Diagnostics {
  journal_mode: string;
  foreign_keys: boolean;
  synchronous: number;
  busy_timeout: number;
  fts5: boolean;
  rollback: boolean;
  reopened: boolean;
  same_day_fsrs: boolean;
  word_count: number;
}

/**
 * @description 正式数据库的可用性及受阻原因。
 * @param 不适用，数据接口。
 * @throws 不适用。
 */
export interface StorageStatus {
  state: 'ready' | 'blocked';
  schema_version: number | null;
  data_directory: string;
  library_count: number | null;
  word_count: number | null;
  error: { code: string; message: string; retryable: boolean } | null;
}

export const desktop = isTauri();
export const api = {
  /**
   * @description 读取原生后端运行环境。
   * @param 无。
   * @throws IPC 失败时拒绝 Promise。
   */
  health: () => invoke<Health>('health'),
  /**
   * @description 验证合成数据的持久化及调度能力。
   * @param 无。
   * @throws IPC 或诊断失败时拒绝 Promise。
   */
  diagnostics: () => invoke<Diagnostics>('run_diagnostics'),
  /**
   * @description 按游标检索合成词条。
   * @param query 用户输入的日文或中文关键词。
   * @param after 上一页末尾词条标识，零表示第一页。
   * @throws IPC、输入校验或数据库失败时拒绝 Promise。
   */
  search: (query: string, after = 0) => invoke<SearchPage>('search_words', { query, after, limit: 100 }),
  /**
   * @description 预览四级反馈的初始调度结果。
   * @param 无。
   * @throws IPC 或调度计算失败时拒绝 Promise。
   */
  scheduler: () => invoke<Rating[]>('scheduler_preview'),
  /**
   * @description 读取正式存储状态，业务受阻信息包含在结果中。
   * @param 无。
   * @throws IPC 任务失败时拒绝 Promise。
   */
  storage: () => invoke<StorageStatus>('storage_status'),
};

/**
 * @description 将 IPC 或运行时错误转换为用户可见消息。
 * @param error 外部调用返回的错误。
 * @throws 无。
 */
export function errorMessage(error: unknown): string {
  if (typeof error === 'object' && error !== null && 'message' in error) return String(error.message);
  return String(error);
}
