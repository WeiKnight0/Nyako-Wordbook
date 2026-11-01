import { invoke } from '@tauri-apps/api/core';
import { nextTick } from 'vue';
import { api } from './api';
import type { useVocabulary } from './composables/useVocabulary';

async function painted() {
  await nextTick();
  await new Promise<void>(resolve => requestAnimationFrame(() => requestAnimationFrame(() => resolve())));
}

function percentiles(values: number[]) {
  const sorted = [...values].sort((a, b) => a - b);
  return { samples: sorted.length, p50_ms: sorted[Math.ceil(sorted.length * .5) - 1], p95_ms: sorted[Math.ceil(sorted.length * .95) - 1], max_ms: sorted.at(-1) };
}

/** @description 在显式启用的隔离构建中测量生产查询和可见列表绘制。 @param state 当前页面使用的正式词汇状态。 @throws IPC、隐藏页面或查询失败时报告错误并停止验收。 */
export async function measureAcceptance(state: ReturnType<typeof useVocabulary>) {
  if (!(await api.health()).acceptance_measure) return;
  const sample = (value: Record<string, unknown>) => invoke('acceptance_sample', { sample: value });
  const initial = structuredClone(JSON.parse(JSON.stringify(state.query.value)));
  async function search(append = false) {
    if (document.hidden) throw new Error('验收页面必须保持可见');
    const started = performance.now();
    await state.search(append);
    if (state.error.value) throw new Error(state.error.value);
    await painted();
    return performance.now() - started;
  }
  async function group(name: string, append = false) {
    const times: number[] = [];
    for (let i = 0; i < 100; i++) {
      if (append) await search();
      times.push(await search(append));
    }
    await sample({ case: name, ...percentiles(times), total: state.total.value, rows: state.words.value.length });
  }
  await painted();
  await sample({ case: 'startup/navigation', elapsed_ms: performance.now(), rows: state.words.value.length, visibility: document.visibilityState, user_agent: navigator.userAgent, longtask_supported: PerformanceObserver.supportedEntryTypes.includes('longtask') });
  for (const sort of ['updated_at', 'kanji', 'kana', 'due_at']) {
    state.query.value = { ...initial, sort };
    await group(`ui/first/${sort}`);
    await group(`ui/cursor/${sort}`, true);
  }
  const keywords = Array.from({ length: 100 }, (_, i) => [
    `工作学习${i}`, `ハタラク${i}`, `はたらく${i} 工作`, `罕见备注 ${i * 1000}`, `不存在词条${i}`,
  ][i % 5]!);
  const keywordTimes: number[] = [];
  for (const text of keywords) {
    state.query.value = { ...initial, text };
    const elapsed_ms = await search();
    keywordTimes.push(elapsed_ms);
    await sample({ case: 'keyword', text, elapsed_ms, total: state.total.value });
  }
  await sample({ case: 'keywords/100-distinct', ...percentiles(keywordTimes), distinct: new Set(keywords).size });
  for (const due of [true, false]) {
    state.query.value = { ...initial, library_id: state.libraries.value.find(library => library.name === '基准主库')!.id, jlpt: 'N5', due, pos: 'noun' };
    await group(`ui/compound/due-${due}`);
  }
  state.query.value = { ...initial };
  await search();
  const viewport = document.querySelector<HTMLElement>('[aria-label="正式词条列表"]')!;
  for (const loaded of [100, 1000, 5000]) {
    while (state.words.value.length < loaded && state.cursor.value) await search(true);
    state.selected.value = loaded === 100 ? [] : state.words.value.slice(0, 500).map(({ id, revision }) => ({ id, revision }));
    viewport.scrollTop = 0;
    await painted();
    const frames: number[] = [];
    const longtasks: number[] = [];
    const observer = PerformanceObserver.supportedEntryTypes.includes('longtask') ? new PerformanceObserver(entries => longtasks.push(...entries.getEntries().map(entry => entry.duration))) : undefined;
    observer?.observe({ type: 'longtask' });
    const start = performance.now();
    let previous: number | undefined;
    await new Promise<void>((resolve, reject) => {
      function frame(now: number) {
        if (document.hidden || !viewport.isConnected) { reject(new Error('滚动验收页面已隐藏或卸载')); return; }
        if (previous !== undefined) frames.push(now - previous);
        previous = now;
        const phase = ((now - start) % 6000) / 3000;
        viewport.scrollTop = (phase <= 1 ? phase : 2 - phase) * (viewport.scrollHeight - viewport.clientHeight);
        if (now - start < 30_000) requestAnimationFrame(frame);
        else resolve();
      }
      requestAnimationFrame(frame);
    }).finally(() => {
      longtasks.push(...(observer?.takeRecords().map(entry => entry.duration) ?? []));
      observer?.disconnect();
    });
    await sample({ case: `scroll/${loaded}`, duration_ms: performance.now() - start, rows: state.words.value.length, selected: state.selected.value.length, frame_intervals: percentiles(frames), frames_over_100ms: frames.filter(time => time > 100).length, longtasks: observer ? percentiles(longtasks) : null, visibility: document.visibilityState });
  }
  state.selected.value = [];
  state.query.value = initial;
  await search();
  await sample({ case: 'complete' });
}
