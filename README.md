# NyakoWordbook

本地日语单词本，已进入 M1-A 正式数据底座开发，M0 剩余平台验收并行保留。现有应用提供正式数据库初始化、校验迁移、迁移前快照、后台串行事务和单实例控制；保留独立日文检索/FSRS 验证。尚无正式词库编辑、导入或学习记录。

前端 `App.vue` 负责页面组装，`composables/useWordSearch.ts` 管理检索时序与列表选择，`composables/useDiagnostics.ts` 管理验证流程，`components/StoragePanel.vue` 展示正式库状态。后端 `storage/service.rs` 承担串行请求，`storage/settings.rs` 管理事务及设置规则，`storage/migrations.rs` 负责校验、快照与升级；`StorageState` 以互斥状态保存就绪服务或失败原因。

## 本地运行

需要 Node 24、npm、Rust stable，以及 Tauri 2 对应平台的桌面构建依赖。Windows 需要 MSVC C++ Build Tools、Windows SDK、WebView2；Linux 依赖见 `.github/workflows/desktop.yml`。

```sh
npm ci
npm run desktop:dev
```

`npm run dev` 仅运行浏览器界面，会明确提示后端不可用，不产生模拟通过结果。

## 检查与打包

```sh
npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
npm run test:rust -- --locked
npm run desktop:build
npm run benchmark
```

Windows 可执行文件：`src-tauri/target/release/nyako-wordbook.exe`。已生成 NSIS 安装包 `src-tauri/target/release/bundle/nsis/NyakoWordbook_0.1.0_x64-setup.exe`（约 2.69 MiB），可通过 `npm run desktop:build -- --bundles nsis` 重建。Rust 库已改名以消除 PDB 冲突；基准程序只在启用 `benchmark` feature 时构建，不随应用安装。MSI 标准重试仍在官方 WiX 下载阶段停滞，尚未通过；NSIS 成功不代表 MSI 通过。首次打包需要联网下载打包工具。

Windows 数据根目录通常为 `%APPDATA%/io.nyako.wordbook/`，以界面实际显示为准。`data/main.sqlite3` 为正式库（当前 Schema 2，初始零词库/零词条）；`diagnostics/probe.sqlite3` 独立保存 2,000 条合成数据。`backups/` 保存既有库升级前在线快照，`media/`、`staging/` 为后续功能预留。基准在系统临时目录生成独立数据库并清理。

启动先只读校验既有库，拒绝高版本、未知库、损坏文件和迁移记录不一致；不会自动覆盖或创建空库替代。新库先在同目录临时文件完成迁移再落盘。既有库升级先备份，迁移失败事务回滚。当前快照仅为数据库迁移保护，不是含媒体的完整 `.nyakobak` 备份/恢复功能。

代码风格重构后已通过 11 个 Rust 测试、7 个前端测试及全 feature Clippy；发行窗口显示正式库 Schema 2/零数据，重启保持，第二次启动退出并聚焦已有窗口。原生故障隔离验收现已通过：高版本显示 `SCHEMA_TOO_NEW`，损坏文件显示 `DB_CORRUPT`，独立正常目录显示 Schema 2；故障文件保持原值，未触碰真实库。

仅验收构建启用 `acceptance` feature：先创建独立绝对目录及其中的 `.nyako-acceptance` 标记文件，再设置进程环境变量 `NYAKO_ACCEPTANCE_ROOT`，执行 `npm run tauri -- build --no-bundle --features acceptance` 并启动生成的 exe。正式库、诊断库和 WebView 缓存均使用该目录，应用标识为 `io.nyako.wordbook.acceptance`；缺少目录参数或标记时拒绝启动，不回退真实目录。普通 `npm run desktop:build -- --bundles nsis` 不启用此入口，已验证即使设置该环境变量也仍使用默认数据目录。验收构建与普通构建共享 target 输出路径，测试后应重新构建普通发行版，避免误分发验收程序。

## 已验证与剩余验收

Windows 上已通过前端类型检查/构建、Rust Clippy 和两个业务集成测试；真实发行窗口已完成 IPC 连接、数据库检查、FSRS 展示、片假名加中文 AND 搜索、列表方向键选择和退出重开。SQLite 为 bundled 3.54.1，rusqlite 0.41.1，FSRS 6 使用 fsrs-rs 6.6.2（BSD-3-Clause）。

FSRS 初始参考向量来自 fsrs-rs 6.6.2 的 `src/inference.rs` 中 `next_states` 文档示例；固定保持率 0.9、无随机抖动。同日经过时间传 0 天，分钟步骤只覆盖 due_at。正式状态机、日切与持久化事件将在 M2 实现。

20 万条简化合成数据首次写入 0.95 秒；每查询 100 次，后端 P95：`工` 6.02 ms、`工作` 2.55 ms、`ハタラク` 7.96 ms、`はたらく 工作` 11.88 ms、无结果查询 2.65 ms。该数据没有正式多义项/例句/百万历史事件，且不含 IPC/渲染耗时，不能视为完整性能验收。

本轮 NSIS 已在工作区隔离目录完成当前用户安装、安装版原生 IPC 检查、片假名加中文搜索、分页、关闭重开及卸载。卸载返回 0，应用诊断数据保留，卸载注册项已移除。安装目录中只有应用和卸载器。启用进程级 2 倍 WebView 缩放后上述核心检查可用，但不等同于真实系统 200% 缩放的视觉/可点击验收。

`npm test` 执行 Vue 事件回归，覆盖组合输入、过期响应与失败、验证错误展示、分页选择、卸载取消和正式存储受阻提示；这些使用模拟后端，不替代原生或真实输入法验证。前端测试依赖审计当前为 0 漏洞。Rust 全 feature 检查命令为 `cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings` 和 `npm run test:rust -- --locked --all-features`。

跨平台工作流已配置但尚未远程运行，M0 总体未完成。剩余人工验收：

1. Windows 系统缩放设为 200%，在最小窗口检查搜索、列表、分页及验证按钮可见且可点击；本轮没有修改系统显示设置。
2. 使用真实日文 IME 输入 `hataraku`，转换并确认 `働く`；组合输入期间不查询，确认后正常查询，数字/空格/J/K 不触发列表导航。本机日语语言项未配置输入法，未安装语言包。
3. 在 macOS/Linux 实际执行工作流对应构建，并安装启动产物，验证后端、日文搜索、FSRS 和重启。
4. 网络条件允许时重试 `npm run desktop:build -- --bundles msi` 并完成 MSI 验收。

完整产品路线见 [规格总览](docs/code_artifact.md)。
