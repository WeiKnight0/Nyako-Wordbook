# NyakoWordbook

本地日语单词本。正式词条采用固定十列可表达的写法、假名、释义、词性、音调、JLPT、例句及备注；词条可有多个义项、例句、数字音调和共享词库。支持词库管理、检索、筛选、分页、批量成员操作、XLSX/JSON 导入导出与 `.nyakobak` 新库备份恢复；正式学习评分尚未开放。桌面启动时先显示非白色等待界面，数据库在后台检查，检查失败会显示受阻原因。正式库直接使用应用数据目录下的 `data/main.sqlite3`，旧版数据已删除，不使用版本子目录，也不提供旧库迁入操作。

正式页面由 `views/LibraryView.vue` 组装，`composables/useVocabulary.ts` 管理查询与过期响应，`components/WordEditor.vue` 保存完整聚合。后端 `storage/words.rs` 保留子记录身份和已有卡片，`storage/query.rs` 实现过滤、排序与分页，所有操作通过串行存储线程执行。保存失败保留草稿并按稳定 ID 核对数据库；关闭未保存表单提供保存、放弃和继续编辑。

录入上限：写法/假名 1,000 字符，义项与例句各 100 条、单项 10,000 字符，备注 100,000 字符，音调 20 条、所属词库 500 个。词库名称 100 字符、描述 10,000 字符。单次批量最多 500 个显式选择的词条，查询变化清空选择；不自动操作全部匹配结果。

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
```

Windows 可执行文件：`src-tauri/target/release/nyako-wordbook.exe`。已生成 NSIS 安装包 `src-tauri/target/release/bundle/nsis/NyakoWordbook_0.1.0_x64-setup.exe`（约 3.78 MiB），可通过 `npm run desktop:build -- --bundles nsis` 重建。Rust 库已改名以消除 PDB 冲突；基准程序只在启用 `benchmark` feature 时构建，不随应用安装。MSI 标准重试仍在官方 WiX 下载阶段停滞，尚未通过；NSIS 成功不代表 MSI 通过。首次打包需要联网下载打包工具。

Windows 数据根目录通常为 `%APPDATA%/io.nyako.wordbook/`，以界面实际显示为准。正式库为 `data/main.sqlite3`（新标识、Schema 2），不使用版本子目录；本机旧版数据已按要求删除，原 `v3/` 中的新版数据移到根目录。`diagnostics/probe.sqlite3` 为独立诊断库，`backups/` 保存备份，`staging/` 保存导入暂存和恢复候选。

启动先显示静态非白色等待界面，同时在后台初始化新版库；就绪后显示词库，失败则显示受阻原因。新版库首次使用时创建空库，此后拒绝高版本、未知库、损坏文件及迁移记录不一致，不会自动覆盖故障库。`.nyakobak` 只备份和恢复新版数据库（包含已有学习状态），恢复前自动备份当前数据并在下次启动切换。旧版库不会参与这一流程。

本轮通过 36 个 Rust 测试、32 个前端测试、类型检查、构建、Rust 格式检查和默认 feature Clippy；隔离原生窗口通过新版 JSON 导入、词条检索、所选词条 XLSX/JSON 导出、Excel 重导入、完整备份、恢复重启与再次检索。尚未在 Microsoft Excel 本体中检查文件，也未对启动首帧进行逐帧视觉测量；历史性能结果不能直接套用新版空库。详细记录见 `docs/specs/06-delivery-validation.md`。

使用入口：启动应用后点击“数据管理”，可先下载单表 Excel 模板，填写固定十列（词条标识、写法、假名、释义、词性、音调、JLPT、日文例句、例句中文翻译、备注），其中写法、假名、释义必填；多义项和多例句在单元格内逐行写 `1. …`、`2. …`，多音调用 `0/1/2` 连接。选择 `.xlsx` 文件并查看样本，确认来源标识后选择现有词库或填写新词库名称，点击“全量校验并预览”，处理冲突后“确认并原子提交”，返回词库即可检索。编辑后重新导入保留词条标识；更新时选择复用及替换字段。超出 Excel 单元格限制或文本与编号格式冲突时会明确拒绝导出，可改用 JSON。CSV 不再作为导入导出格式。导出可在词库选择词条后点击“导出所选词条”，或在数据管理选择整个词库。完整备份使用“创建完整备份”；恢复先“选择备份并完整校验”，确认覆盖后安排恢复，关闭并重新启动应用完成切换。

仅验收构建启用 `acceptance` feature：先创建独立绝对目录及其中的 `.nyako-acceptance` 标记文件，再设置进程环境变量 `NYAKO_ACCEPTANCE_ROOT`，执行 `npm run tauri -- build --no-bundle --features acceptance` 并启动生成的 exe。正式库、诊断库和 WebView 缓存均使用该目录，应用标识为 `io.nyako.wordbook.acceptance`；缺少目录参数或标记时拒绝启动，不回退真实目录。普通 `npm run desktop:build -- --bundles nsis` 不启用此入口，已验证即使设置该环境变量也仍使用默认数据目录。验收构建与普通构建共享 target 输出路径，测试后应重新构建普通发行版，避免误分发验收程序。

## 已验证与剩余验收

Windows 上已通过前端类型检查/构建、Rust Clippy 和两个业务集成测试；真实发行窗口已完成 IPC 连接、数据库检查、FSRS 展示、片假名加中文 AND 搜索、列表方向键选择和退出重开。SQLite 为 bundled 3.54.1，rusqlite 0.41.1，FSRS 6 使用 fsrs-rs 6.6.2（BSD-3-Clause）。

FSRS 初始参考向量来自 fsrs-rs 6.6.2 的 `src/inference.rs` 中 `next_states` 文档示例；固定保持率 0.9、无随机抖动。同日经过时间传 0 天，分钟步骤只覆盖 due_at。正式状态机、日切与持久化事件将在 M2 实现。

正式基准覆盖 20 万词、每词 2 义项/2 例句/3 标签、20 万卡和 100 万事件，包含共享与分散标签两种分布。最终独立原生 IPC→Vue 绘制中，100 个不同中日关键词 P95 77 ms；四种排序首屏 P95 40–180 ms，后续分页 30–180 ms。稀疏标签任一匹配后端 P95 从约 219–225 ms 降至 39–40 ms。100/1000/5000 行各 30 秒往返滚动均无长任务，最大帧间隔 20.1 ms。高覆盖率共享标签全部匹配后端约 280 ms、分散任一标签复合查询端到端 250 ms，仍超 200 ms；完整启动校验仍超 3 秒，M1-B 尚未关闭。数据形状、缓存限制与最终启动结果见 `docs/specs/06-delivery-validation.md`。

`benchmark` feature 的正式夹具仍引用旧标签结构，尚未迁到新版空库；不要用它验收当前版本的启动或查询性能。上述旧版性能数字仅为历史记录。

可重跑 Windows 验收脚本保存在 `scripts/`。先构建 acceptance exe，再用 PowerShell 执行 `windows-acceptance.ps1 -Executable <绝对exe路径> -DataRoot <绝对标记目录> -Scenario cold -Samples 20`；`measure` 场景运行排序/分页各 100 次、100 个不同关键词、标签/复合筛选和三档行数的 30 秒往返滚动，必须保持窗口可见。输出为目录内的 JSON/JSONL，重复运行 measure 前须归档旧 `frontend.jsonl`。`memory` 场景记录整个进程树 10 秒内的采样峰值，也支持空库。`NYAKO_ACCEPTANCE_TIMING=1` 记录后端启动分段。`windows-native-regression.ps1` 验证隔离目录中的鼠标操作、词条创建删除、草稿及关闭流程，`-DoubleScale` 使用进程级两倍缩放；`windows-install-regression.ps1` 在无既有安装时验证普通 NSIS 的安装/单实例/重开/卸载，`windows-storage-regression.ps1` 核对隔离故障库的错误码和拒绝前后文件哈希。测试会写入指定的隔离目录，请为测试新建目录并顺序执行，避免性能测量互相干扰。

前轮 NSIS 已在工作区隔离目录完成当前用户安装、启动、单实例、关闭重开及卸载，数据库哈希保持；本轮最终安装包回归结果见验收记录。真实词条增删改、标签、分页与二次确认在独立 acceptance exe 中验证。1000×700 窗口加进程级 2 倍 WebView 缩放下，四个主要操作已核对边界位于窗口内并实际鼠标点击；关闭保存/放弃/继续和保存失败保留草稿通过。完整视觉截断与真实系统 200% 缩放仍待验收。

`npm test` 执行 Vue 事件回归，覆盖组合输入、过期响应与失败、验证错误展示、分页选择、卸载取消和正式存储受阻提示；这些使用模拟后端，不替代原生或真实输入法验证。前端测试依赖审计当前为 0 漏洞。Rust 全 feature 检查命令为 `cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings` 和 `npm run test:rust -- --locked --all-features`。

跨平台工作流已配置但尚未远程运行，M0 总体未完成。剩余人工验收：

1. Windows 系统缩放设为 200%，在最小窗口检查搜索、列表、分页及验证按钮可见且可点击；本轮没有修改系统显示设置。
2. 使用真实日文 IME 输入 `hataraku`，转换并确认 `働く`；组合输入期间不查询，确认后正常查询，数字/空格/J/K 不触发列表导航。本机日语语言项未配置输入法，未安装语言包。
3. 在 macOS/Linux 实际执行工作流对应构建，并安装启动产物，验证后端、日文搜索、FSRS 和重启。
4. 网络条件允许时重试 `npm run desktop:build -- --bundles msi` 并完成 MSI 验收。

完整产品路线见 [规格总览](docs/code_artifact.md)。
