<script setup lang="ts">
import { nextTick, onBeforeUnmount, onMounted, ref } from 'vue';
import { desktop, errorMessage } from './api';
import StoragePanel from './components/StoragePanel.vue';
import { useDiagnostics } from './composables/useDiagnostics';
import { useWordSearch } from './composables/useWordSearch';

const error = ref('');
const searchInput = ref<HTMLInputElement>();
const { storage, health, diagnostics, ratings, verifying, verify, initialize } = useDiagnostics();
const {
  words, query, loading, cursor, elapsed, active, viewport, rowHeight,
  start, visible, selected, search, beginComposition, endComposition,
  scheduleSearch, select, scroll, navigate,
} = useWordSearch(error);
const labels = ['忘记', '模糊', '认识', '太简单'];

async function runVerification() {
  error.value = '';
  try {
    await verify();
  } catch (cause) {
    error.value = errorMessage(cause);
  }
}

function globalKey(event: KeyboardEvent) {
  if (event.isComposing || event.repeat) return;
  if (!(event.ctrlKey || event.metaKey) || event.key !== 'f') return;
  event.preventDefault();
  searchInput.value?.focus();
}

onMounted(async () => {
  window.addEventListener('keydown', globalKey);
  if (!desktop) {
    error.value = '当前为浏览器预览。请使用 npm run desktop:dev 启动真实桌面后端；此处不提供模拟验证结果。';
    return;
  }
  try {
    await initialize();
    await nextTick();
    await search();
  } catch (cause) {
    error.value = errorMessage(cause);
  }
});
onBeforeUnmount(() => window.removeEventListener('keydown', globalKey));
</script>

<template>
  <div class="shell">
    <aside class="sidebar">
      <div class="brand">
        <span class="brand-icon">単</span>
        <div>Nyako<span class="brand-sub">WORDBOOK</span></div>
      </div>
      <div class="section-label">开发工作台</div>
      <a class="nav active" href="#workspace">技术验证 <span>M0</span></a>
      <div class="sidebar-note">本地优先的日语单词本<p>当前为工程验证阶段。列表为合成数据，不是个人学习记录。</p></div>
      <div class="offline"><span></span>核心验证在本地运行</div>
    </aside>
    <main id="workspace">
      <header>
        <div>
          <div class="eyebrow">工程初始化 / M0</div>
          <h1>让每一个基础环节可验证。</h1>
          <p class="muted">SQLite 持久化 · 日文检索 · FSRS 调度</p>
        </div>
        <button :disabled="!desktop || verifying" @click="runVerification">
          {{ verifying ? '验证中…' : '重新运行验证' }}
        </button>
      </header>
      <div v-if="error" class="error" role="alert">{{ error }}</div>
      <section class="status-grid" aria-label="环境状态">
        <article>
          <span>桌面后端</span>
          <strong>{{ health ? '已连接' : '等待连接' }}</strong>
          <small>{{ health ? `${health.platform} · v${health.app_version}` : '需要 Tauri 原生环境' }}</small>
        </article>
        <article>
          <span>SQLite</span>
          <strong>{{ diagnostics?.journal_mode.toUpperCase() ?? '待验证' }}</strong>
          <small>{{ health ? `v${health.sqlite_version} · FULL 同步` : '真实文件数据库' }}</small>
        </article>
        <article>
          <span>检索数据</span>
          <strong>{{ diagnostics?.word_count.toLocaleString() ?? '—' }} <em>词</em></strong>
          <small>合成数据 · FTS5 trigram</small>
        </article>
        <article>
          <span>调度算法</span>
          <strong>FSRS 6</strong>
          <small>{{ diagnostics?.same_day_fsrs ? '同日复习检查通过' : '固定参数 · 保持率 90%' }}</small>
        </article>
      </section>
      <StoragePanel :storage="storage" />
      <section class="workspace">
        <div class="list-panel">
          <div class="panel-heading"><h2>词汇检索原型</h2><span class="badge">验证数据</span></div>
          <div class="search">
            <span>検索</span>
            <input
              ref="searchInput"
              v-model="query"
              aria-label="检索日文或中文"
              placeholder="搜索汉字、假名或中文…"
              @input="scheduleSearch"
              @compositionstart="beginComposition"
              @compositionend="endComposition"
            >
            <kbd>Ctrl / ⌘ F</kbd>
          </div>
          <div class="table-head"><span>写法 / 读音</span><span>中文释义</span></div>
          <div
            ref="viewport"
            class="word-viewport"
            role="listbox"
            aria-label="检索结果"
            :aria-activedescendant="selected ? `word-${selected.id}` : undefined"
            tabindex="0"
            @scroll="scroll"
            @keydown="navigate"
          >
            <div :style="{ height: `${words.length * rowHeight}px`, position: 'relative' }">
              <div :style="{ transform: `translateY(${start * rowHeight}px)` }">
                <div
                  v-for="(word, index) in visible"
                  :id="`word-${word.id}`"
                  :key="word.id"
                  class="word-row"
                  :class="{ selected: start + index === active }"
                  role="option"
                  :aria-selected="start + index === active"
                  @click="select(start + index)"
                >
                  <div>
                    <strong lang="ja">{{ word.kanji }}</strong>
                    <small lang="ja">{{ word.kana }}</small>
                  </div>
                  <span>{{ word.meaning }}</span>
                </div>
              </div>
            </div>
            <p v-if="!words.length && !loading" class="empty">{{ desktop ? '没有匹配结果，请调整关键词。' : '启动桌面应用后查看真实数据。' }}</p>
          </div>
          <footer>
            <span>{{ loading ? '查询中…' : `已加载 ${words.length} 条 · 后端 ${elapsed.toFixed(1)} ms` }}</span>
            <button v-if="cursor !== null" class="text-button" :disabled="loading" @click="search(true)">
              加载下一页
            </button>
          </footer>
        </div>
        <aside class="detail-panel">
          <div class="section-label">当前词条</div>
          <h2 lang="ja">{{ selected?.kanji ?? '等待数据' }}</h2>
          <p class="reading" lang="ja">{{ selected?.kana ?? '—' }}</p>
          <div class="divider"></div>
          <div class="section-label">中文释义</div>
          <p>{{ selected?.meaning ?? '选择左侧词条查看详情。' }}</p>
          <div class="hint">支持 ↑ / ↓ 或 J / K 切换词条。搜索框内保留正常输入，不触发列表热键。</div>
          <div class="checks">
            <h3>数据可靠性检查</h3>
            <p
              v-for="[label, passed] in [['外键约束', diagnostics?.foreign_keys], ['事务回滚', diagnostics?.rollback], ['关闭后重新打开', diagnostics?.reopened], ['FTS5 索引', diagnostics?.fts5]]"
              :key="String(label)"
            >
              <span>{{ label }}</span>
              <b>{{ passed === undefined ? '待验证' : passed ? '通过' : '失败' }}</b>
            </p>
          </div>
        </aside>
      </section>
      <section class="scheduler">
        <div class="panel-heading">
          <div>
            <h2>调度结果预览</h2>
            <p class="muted">初次学习的四级反馈；只计算，不记录学习进度。</p>
          </div>
          <span class="badge">fsrs-rs 6.6.2</span>
        </div>
        <div class="rating-grid">
          <article v-for="rating in ratings" :key="rating.rating">
            <span>{{ rating.rating }} · {{ labels[rating.rating - 1] }}</span>
            <strong>{{ new Date(rating.due_at_ms).toLocaleString('zh-CN') }}</strong>
            <small>稳定性 {{ rating.stability_days.toFixed(4) }} 天 · 难度 {{ rating.difficulty.toFixed(2) }}</small>
          </article>
          <p v-if="!ratings.length" class="muted">连接原生后端后显示实际计算结果。</p>
        </div>
      </section>
      <p class="bottom-note">技术验证不等于完整产品验收。跨平台启动、发行安装与性能基准分别记录。</p>
    </main>
  </div>
</template>
