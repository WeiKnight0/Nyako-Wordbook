# NyakoWordbook

本地日语单词本。正式词条采用固定十列可表达的写法、假名、释义、词性、音调、JLPT、例句及备注；释义、日文例句和中文翻译分别为多行文本，音调为自由文本，词条可属于多个共享词库。支持词库管理、检索、筛选、分页、批量成员操作、XLSX/JSON 导入导出与 `.nyakobak` 新库备份恢复；正式学习评分尚未开放。桌面启动时先显示非白色等待界面，数据库在后台检查，检查失败会显示受阻原因。正式库直接使用应用数据目录下的 `data/main.sqlite3`，旧版数据已删除，不使用版本子目录，也不提供旧库迁入操作。

正式页面由 `views/LibraryView.vue` 组装，`composables/useVocabulary.ts` 管理查询与过期响应，`components/WordEditor.vue` 保存词条文本和成员关系。后端 `storage/words.rs` 直接保存文本并保留词条身份和已有卡片，`storage/query.rs` 实现过滤、排序与分页，所有操作通过串行存储线程执行。保存失败保留草稿并按稳定 ID 核对数据库；关闭未保存表单提供保存、放弃和继续编辑。

浏览词条时，宽窗口默认显示紧凑词目索引和内嵌阅读区，可切换为完整列表；窄窗口打开独立阅读页，返回后保留活动词条和列表位置。阅读区提供上一词/下一词与页末续读，原样展示多行释义、独立日中例句、已记录的音调、备注和所属词库。列表使用 ↑/↓ 或 J/K 选择，Enter 聚焦阅读；底部提供每 100 条的上一页/下一页导航，返回已加载页无需重新查询，滚动仍可浏览全部已加载词条。阅读区使用 ←/→ 切词、Esc 返回列表。

动词类别保留五段、一段，サ变与カ变统一为“サ变/カ变动词”（`irregular_verb`）；自他属性独立可选。单条删除位于编辑词条窗口底栏，确认显示影响范围，取消或失败保留草稿；列表保留批量删除。

录入上限：写法/假名 1,000 字符，释义、日文例句和中文翻译各 100,000 字符，备注 100,000 字符，音调 1,000 字符、所属词库 500 个。词库名称 100 字符、描述 10,000 字符。单次批量最多 500 个显式选择的词条，查询变化清空选择；不自动操作全部匹配结果。

前端 `App.vue` 缓存词库与数据管理页面，普通切页保留查询、已加载分页、勾选和列表位置；导入成功后通知词库重新查询并核对所选版本。`composables/useVocabulary.ts` 管理正式检索与选择，`composables/useWordSearch.ts` 和 `composables/useDiagnostics.ts` 仅用于技术诊断，`components/StoragePanel.vue` 展示正式库状态。后端 `storage/service.rs` 承担串行请求，`storage/settings.rs` 管理事务及设置规则，`storage/migrations.rs` 负责当前模型校验与新库初始化；`StorageState` 以互斥状态保存就绪服务或失败原因。

## 本地运行

需要 Node 24、npm、Rust stable，以及 Tauri 2 对应平台的桌面构建依赖。Windows 需要 MSVC C++ Build Tools、Windows SDK、WebView2；Linux 依赖见 `.github/workflows/desktop.yml`。

```sh
npm ci
npm run desktop:dev
```

`npm run dev` 运行带隔离样例的浏览器界面，可检索和浏览，保存及文件交换不可用；正式数据操作使用桌面版。

## 检查与打包

```sh
npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
npm run test:rust -- --locked
npm run desktop:build
```

Windows 可执行文件：`src-tauri/target/release/nyako-wordbook.exe`。已生成 NSIS 安装包 `src-tauri/target/release/bundle/nsis/NyakoWordbook_0.1.0_x64-setup.exe`（本轮约 3.74 MiB），可通过 `npm run desktop:build -- --bundles nsis` 重建。Rust 库已改名以消除 PDB 冲突；基准程序只在启用 `benchmark` feature 时构建，不随应用安装。MSI 标准重试仍在官方 WiX 下载阶段停滞，尚未通过；NSIS 成功不代表 MSI 通过。首次打包需要联网下载打包工具。

Windows 数据根目录通常为 `%APPDATA%/io.nyako.wordbook/`，以界面实际显示为准。正式库为 `data/main.sqlite3`（文本模型新标识、Schema 1），不使用版本子目录；不提供旧数据库迁移或兼容读取，开发阶段旧库可清理后重新启动创建。`diagnostics/probe.sqlite3` 为独立诊断库，`backups/` 保存备份，`staging/` 保存导入暂存和恢复候选。

启动先显示静态非白色等待界面，同时在后台初始化新版库；就绪后显示词库，失败则显示受阻原因。新版库首次使用时创建空库，此后启动使用 `quick_check`，并拒绝高版本、未知库、快速检查发现的损坏及迁移记录不一致，不会自动覆盖故障库；备份与恢复继续执行完整校验。`.nyakobak` 只备份和恢复新版数据库（包含已有学习状态），恢复前自动备份当前数据并在下次启动切换。旧版库不会参与这一流程。

当前版本的测试与原生验收结果按日期记录在 `docs/specs/06-delivery-validation.md`。历史记录仅证明对应版本的场景；尚未在 Microsoft Excel 本体中检查文件，也未对启动首帧进行逐帧视觉测量，历史性能结果不能直接套用当前数据模型。

使用入口：启动应用后点击“数据管理”，可先下载单表 Excel 模板，填写固定十列（词条标识、写法、假名、释义、词性、音调、JLPT、日文例句、例句中文翻译、备注），其中写法、假名、释义必填；释义和例句单元格可直接换行、留空行或自行编号，程序不解析编号，不要求日中例句数量相同；音调可写 `0/2` 或文字。选择 `.xlsx` 文件并查看样本，确认来源标识后选择现有词库或填写新词库名称，点击“全量校验并预览”，处理冲突后“确认并原子提交”，返回词库即可检索。预览显示整批匹配分类与待确认身份数量，可定位未处理项；字段核对使用中文对比表，只有文件提供的字段参与更新。修改文件或来源须重新校验。编辑后重新导入保留词条标识；应用新导出的 XLSX 通过工作簿元数据携带来源身份，普通表和模板使用手填来源，旧文件或元数据被第三方删除时须填写原来源。更新时选择复用及替换字段。超出 Excel 单元格限制时会明确拒绝导出，可改用 JSON。CSV 不再作为导入导出格式。导出可在词库选择词条后点击“导出所选词条”，或在数据管理选择整个词库。完整备份使用“创建完整备份”；恢复先“选择备份并完整校验”，确认覆盖后安排恢复，关闭并重新启动应用完成切换。

仅验收构建启用 `acceptance` feature：先创建独立绝对目录及其中的 `.nyako-acceptance` 标记文件，再设置进程环境变量 `NYAKO_ACCEPTANCE_ROOT`，执行 `npm run tauri -- build --no-bundle --features acceptance` 并启动生成的 exe。正式库、诊断库和 WebView 缓存均使用该目录，应用标识为 `io.nyako.wordbook.acceptance`；缺少目录参数或标记时拒绝启动，不回退真实目录。普通 `npm run desktop:build -- --bundles nsis` 不启用此入口，已验证即使设置该环境变量也仍使用默认数据目录。验收构建与普通构建共享 target 输出路径，测试后应重新构建普通发行版，避免误分发验收程序。

## 已验证与剩余验收

Windows 上已通过前端类型检查/构建、Rust Clippy 和两个业务集成测试；真实发行窗口已完成 IPC 连接、数据库检查、FSRS 展示、片假名加中文 AND 搜索、列表方向键选择和退出重开。SQLite 为 bundled 3.54.1，rusqlite 0.41.1，FSRS 6 使用 fsrs-rs 6.6.2（BSD-3-Clause）。

FSRS 初始参考向量来自 fsrs-rs 6.6.2 的 `src/inference.rs` 中 `next_states` 文档示例；固定保持率 0.9、无随机抖动。同日经过时间传 0 天，分钟步骤只覆盖 due_at。正式状态机、日切与持久化事件将在 M2 实现。

正式基准覆盖 20 万词、每词 2 义项/2 例句/3 标签、20 万卡和 100 万事件，包含共享与分散标签两种分布。最终独立原生 IPC→Vue 绘制中，100 个不同中日关键词 P95 77 ms；四种排序首屏 P95 40–180 ms，后续分页 30–180 ms。稀疏标签任一匹配后端 P95 从约 219–225 ms 降至 39–40 ms。100/1000/5000 行各 30 秒往返滚动均无长任务，最大帧间隔 20.1 ms。高覆盖率共享标签全部匹配后端约 280 ms、分散任一标签复合查询端到端 250 ms，仍超 200 ms；完整启动校验仍超 3 秒，M1-B 尚未关闭。数据形状、缓存限制与最终启动结果见 `docs/specs/06-delivery-validation.md`。

`benchmark` feature 的正式夹具已迁到当前文本 Schema 1，标记版本为 text-v1。`shared` 表示两个词库存在共享成员，`distributed` 表示主库与次库互斥分布；每词多行释义、日中例句文本、五事件，不再生成标签。可用 `npm run benchmark -- --formal <绝对标记目录> 256 shared` 验证小夹具；正式性能需另行使用二十万词重测。上述旧版性能数字仅为历史记录。

可重跑 Windows 验收脚本保存在 `scripts/`。先构建 acceptance exe，再用 PowerShell 执行 `windows-acceptance.ps1 -Executable <绝对exe路径> -DataRoot <绝对标记目录> -Scenario cold -Samples 20`；`measure` 场景运行排序/分页各 100 次、100 个不同关键词、当前模型的复合筛选和三档行数的 30 秒往返滚动，必须保持窗口可见。输出为目录内的 JSON/JSONL，重复运行 measure 前须归档旧 `frontend.jsonl`。`memory` 场景记录整个进程树 10 秒内的采样峰值，也支持空库。`NYAKO_ACCEPTANCE_TIMING=1` 记录后端启动分段。`windows-native-regression.ps1` 验证隔离目录中的鼠标操作、词条创建删除、草稿及关闭流程，`-DoubleScale` 使用进程级两倍缩放；`windows-install-regression.ps1` 在无既有安装时验证普通 NSIS 的安装/单实例/重开/卸载，`windows-storage-regression.ps1` 核对隔离故障库的错误码和拒绝前后文件哈希。测试会写入指定的隔离目录，请为测试新建目录并顺序执行，避免性能测量互相干扰。

前轮 NSIS 已在工作区隔离目录完成当前用户安装、启动、单实例、关闭重开及卸载，数据库哈希保持；本轮最终安装包回归结果见验收记录。真实词条增删改、标签、分页与二次确认在独立 acceptance exe 中验证。1000×700 窗口加进程级 2 倍 WebView 缩放下，四个主要操作已核对边界位于窗口内并实际鼠标点击；关闭保存/放弃/继续和保存失败保留草稿通过。完整视觉截断与真实系统 200% 缩放仍待验收。

`npm test` 执行 Vue 事件回归，覆盖组合输入、过期响应与失败、验证错误展示、分页选择、卸载取消和正式存储受阻提示；这些使用模拟后端，不替代原生或真实输入法验证。前端测试依赖审计当前为 0 漏洞。Rust 全 feature 检查命令为 `cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings` 和 `npm run test:rust -- --locked --all-features`。

跨平台工作流已配置但尚未远程运行，M0 总体未完成。剩余人工验收：

1. Windows 系统缩放设为 200%，在最小窗口检查搜索、列表、分页及验证按钮可见且可点击；本轮没有修改系统显示设置。
2. 使用真实日文 IME 输入 `hataraku`，转换并确认 `働く`；组合输入期间不查询，确认后正常查询，数字/空格/J/K 不触发列表导航。本机日语语言项未配置输入法，未安装语言包。
3. 在 macOS/Linux 实际执行工作流对应构建，并安装启动产物，验证后端、日文搜索、FSRS 和重启。
4. 网络条件允许时重试 `npm run desktop:build -- --bundles msi` 并完成 MSI 验收。

完整产品路线见 [规格总览](docs/code_artifact.md)。
