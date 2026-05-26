# 变更记录（Changelog）

本项目变更记录采用 [语义化版本](https://semver.org/lang/zh-CN/) 进行版本管理。

## [1.8.5] - 2026-05-26

### 变更

- 新增 `docs/BUILD.md`，集中说明 macOS 下交叉编译 Windows x64、Linux x64 和 Linux arm64 所需依赖、命令与产物位置
- Makefile 增加 macOS 交叉编译支持：Windows x64 使用 `x86_64-pc-windows-gnu + mingw-w64 + makensis`，Linux x64 / Linux arm64 使用 Docker 构建
- README、PRD 与实现文档同步更新构建说明，统一 macOS 交叉编译入口与注意事项

## [1.8.4] - 2026-05-26

### 修复

- 重新生成应用图标并同步前端、Tauri 和安装包资源，使用更清晰的正式图标替换旧图标素材

## [1.8.3] - 2026-05-26

### 变更

- 达梦 DM 改为仅在 Windows x64、Linux x64 与 Linux arm64 版本中启用；macOS 版本不编译 ODBC 适配器，不展示达梦数据库入口
- 前端数据库类型下拉按当前平台过滤达梦选项，并在 macOS 上显示平台限制说明
- 后端 `create_loader` 在不支持的平台直接拒绝达梦连接配置，防止历史配置或 LLM 识别绕过 UI 限制
- 文档统一标注达梦数据库仅在支持的平台可用

## [1.8.2] - 2026-05-26

### 修复

- 顶部标题栏应用内图标改为复用正式应用图标，移除默认 `GU` 文字占位标识

## [1.8.1] - 2026-05-26

### 修复

- 顶部标题栏右侧版本号改为从前端 `package.json` 读取，避免构建后仍显示写死的 `v0.1`

## [1.8.0] - 2026-05-26

### 变更

- 达梦 DM 驱动目录改为按平台/架构分层：macOS `arm64`、Windows `x64`、Linux `x64` / `arm64`
- `resolve_driver_path()` 同步支持新目录层级，并保留旧目录作为回退
- Makefile 构建命令改为 `build-macos-arm`、`build-windows-x64`、`build-linux-x64`、`build-linux-arm`，`build-all` 覆盖声明矩阵
- 修复 macOS 本机 `make build` 链接阶段找不到 `libodbc` 的问题，构建脚本自动注入 Homebrew unixODBC 搜索路径
- 更新 README、PRD、实现文档与驱动说明，统一新的构建与驱动目录约定

## [1.7.0] - 2026-05-26

### 新增

- Oracle 数据库完整支持：基于 oracle-rs 0.1 纯 Rust TNS 协议驱动，零系统依赖运行（无需 Oracle Instant Client / OCI / ODPI-C）
- 达梦 DM 数据库完整支持：基于 odbc-api 8 ODBC 驱动，驱动随安装包打包分发，最终用户零安装使用
- Oracle / 达梦 DDL 类型映射辅助函数：Oracle 映射 Varchar(n) -> `VARCHAR2(n)`、Decimal(m,n) -> `NUMBER(m,n)`、Int(_) -> `NUMBER(19)`；达梦映射 Varchar(n) -> `VARCHAR(n)`、Decimal(m,n) -> `DECIMAL(m,n)`、Int(_) -> `BIGINT`
- 达梦 ODBC 驱动运行时自动发现与回退机制：`resolve_driver_path()` 依次尝试应用资源目录、开发模式目录、系统注册名称

### 变更

- 完全移除 feature gate：Oracle 与达梦适配器默认参与编译，与 MySQL / PostgreSQL 一致无条件调用
- `src-tauri/tauri.conf.json` 的 `bundle` 段新增 `"resources": ["bundled-drivers/**/*"]`，确保达梦 ODBC 驱动文件随安装包分发
- 新增 `src-tauri/bundled-drivers/` 目录结构与 `odbcinst.ini` 模板，支持三平台驱动打包
- 新增 `src-tauri/bundled-drivers/README.md` 驱动打包说明文档
- Makefile 新增 `bundle-drivers` 目标，显示达梦 ODBC 驱动打包说明

## [1.6.0] - 2026-05-26

### 变更

- 将前端代码迁移至 `frontend/` 子目录：`src/`、`index.html`、`package.json`、`package-lock.json`、`vite.config.ts`、`tsconfig.json`、`tsconfig.node.json`、`tailwind.config.js`、`postcss.config.js`、`components.json` 均移至 `frontend/` 下
- 更新 `src-tauri/tauri.conf.json`：`beforeDevCommand` 改为 `cd ../frontend && npm run dev`、`beforeBuildCommand` 改为 `cd ../frontend && npm run build`、`frontendDist` 改为 `../frontend/dist`
- 重构 `Makefile`：新增 `TAURI_BIN := ./frontend/node_modules/.bin/tauri` 变量，所有 npm 命令改为 `cd frontend && ...` 形式，Tauri CLI 在项目根目录执行以正确定位 `src-tauri/`
- 更新 `.gitignore`：`node_modules` → `frontend/node_modules`、`dist` → `frontend/dist`、`dist-ssr` → `frontend/dist-ssr`

## [1.5.0] - 2026-05-26

### 新增

- 数据库配置持久化保存与复用功能：新增 `SavedDbConfig` 类型（继承 `DatabaseConfig` 并扩展 `id` / `name` 字段），支持将当前数据库连接参数保存为命名配置
- `appStore.ts` 新增 `savedDbConfigs` 状态与 `saveDbConfig` / `deleteSavedDbConfig` / `loadSavedDbConfig` 三个操作方法，数据通过 localStorage（key: `gut-loader-saved-db-configs`）持久化
- `DatabaseConfig.tsx` 连接参数卡片标题行新增配置选择下拉列表（显示已保存配置名称与连接信息摘要）、保存按钮（弹出 Dialog 输入名称后保存）、每条配置旁的删除按钮
- 安全措施：保存时密码字段置空，加载时密码同样为空需用户重新输入
- 前置检查面板进入步骤 3 时自动触发检查，无需手动点击运行按钮

### 变更

- 重构 LLM 配置 UI 布局：将 `LLMConfig` 组件从 LLM 智能识别面板的展开区域中提取出来，改为通过独立的 Dialog 弹窗承载
- LLM 智能识别面板标题栏新增「配置 LLM」按钮（`Settings2` 图标 + outline variant），点击后弹出标题为「LLM 服务配置」的对话框，主体渲染 `<LLMConfig />`，配置仍自动保存到 localStorage
- 当 LLM 未配置（`api_url` 或 `model` 为空）时，配置按钮右上角显示 amber 色小圆点作为状态指示
- 折叠/展开图标拆分为独立按钮，与配置按钮并列；展开后的内容仅保留连接信息文本输入框与「智能识别」按钮
- 跨平台视觉一致性增强：统一字体渲染（antialiased + optimizeLegibility）、自定义滚动条（Webkit + Firefox scrollbar-width）、文本选中高亮色（accent 色 30% 透明度）、Windows 高对比度模式 focus-visible 轮廓兼容

### 修复

- 修正步骤指示器状态显示逻辑：Stepper 组件不再以「当前步骤之前的步骤」判断完成态，改为基于各步骤实际验证条件（`isStepCompleted` 回调）决定是否显示绿色对勾

## [1.3.2] - 2026-05-26

### 修复

- 修复 `run_benchmark` 报告中 Rust 版本显示为空的问题，改为运行时获取 `rustc --version` 实际输出
- 修复小规模数据（如 800 行 employee）解析耗时显示为 0ms、吞吐为 0 行/秒的问题，改用纳秒精度计时并支持浮点毫秒显示
- 将 `is_multiple_of` 替换为 `% 3 == 0`，提升对 Rust 1.73 以下版本的兼容性

## [1.3.1] - 2026-05-26

### 变更

- 替换默认占位图标为专业设计的应用图标：深蓝到青色渐变背景 + 数据库圆柱体 + 琥珀色数据流箭头 + 青色数据粒子，体现"数据加载入库"主题
- 新增 SVG 源文件及完整的多平台图标集：icon.svg、icon.png（1024x1024）、icon.icns（macOS）、icon.ico（Windows）、Square*Logo.png（Windows Store）、StoreLogo.png
- 更新 tauri.conf.json 图标配置，包含所有平台所需图标路径

## [1.3.0] - 2026-05-26

### 新增

- LLM 配置持久化保存：用户配置的 API URL、API Key、模型名称自动保存到 localStorage，下次启动时自动加载
- LLM 配置变更时显示“已自动保存”视觉反馈（emerald 色调，2 秒后淡出）
- 智能识别按钮在 LLM 未配置时禁用，并显示“请先配置 LLM API URL 和模型”提示

### 变更

- LLM 配置默认值改为空字符串（API URL 和模型名称不再预填），保留 placeholder 显示示例值
- LLM 智能识别面板默认展开，无需用户手动点击展开

## [1.2.2] - 2026-05-26

### 修复

- 修复 `make build` 失败：Cargo.toml 缺少 Tauri 主二进制入口，添加 `default-run = "gut-loader"` 与显式 `[[bin]]` 声明指向 `src/main.rs`
- 修复 `cargo clippy` 5 项错误：`batch.rs` 中 match 简化为 `unwrap_or_default()`、`flush_batch` 函数参数过多添加 `#[allow(clippy::too_many_arguments)]`、`flg.rs` 中手动字符比较替换为数组模式 `['(', '（']`
- 修复 `run_benchmark.rs` 3 项 clippy 错误：消除 `let_and_return`、`bool_comparison`、`manual_is_multiple_of` 警告
- 修复 `lib.rs` release 模式下 `tauri::Manager` 未使用导入警告：添加 `#[cfg(debug_assertions)]` 条件编译门控

## [1.2.1] - 2026-05-26

### 文档

- 完善 `README.md` 的构建与开发说明：将「快速开始」中的原 npm 命令替换为统一的 `make install` / `make dev` / `make build` / `make help` 入口
- 新增「构建与开发命令」一级章节，按开发调试、构建打包、测试数据库管理、清理、帮助五大类对全部 19 条 Makefile 命令进行表格化罗列
- 新增「常用工作流示例」子章节，覆盖首次拉取代码、日常开发循环、带数据库的集成测试、多平台构建发布、构建问题重置环境 5 类场景
- `docs/PRD.md` 章节 2.9 已完整覆盖所有 Makefile 命令，本次保留原有内容

## [1.2.0] - 2026-05-26

### 新增

- 完成 Task #12 大文件流式分片处理优化：`src-tauri/src/loader/batch.rs` 重构为双分支加载器，新增 `load_table_streaming` 与 `load_table_inmemory`，`load_table` 根据 dat.gz 文件实际大小自动分发
- 流式加载路径使用 `GzDecoder + BufReader` 逐行读取 + `parse_row_bytes` 逐行解析，每凑齐 `safe_batch_size` 行即调用 `batch_insert` 入库并清空本地缓冲，内存占用始终控制在单批数据量级（默认 1000 行）
- 阈值 `STREAMING_THRESHOLD_BYTES = 100 MB` 导出为公开常量，便于测试与下游引用
- 两条路径均保留断点续传能力（`get_row_count` -> 跳过前 N 行）与进度回调（`mpsc::Sender<LoadProgress>`）
- 详细日志区分：大文件走流式分支时 `info!` 输出文件大小与阈值，小文件走内存分支时 `debug!` 输出，便于运维诊断是否命中预期路径

### 变更

- `src-tauri/src/parser/dat.rs` 中 `is_gzip` 与 `parse_row_bytes` 由私有升为 `pub(crate)`，供加载器复用行解析逻辑以避免重复实现
- `src-tauri/src/loader/mod.rs` 重导出 `load_table` / `load_table_inmemory` / `load_table_streaming` / `STREAMING_THRESHOLD_BYTES`
- `src-tauri/tests/integration_test.rs` 新增 3 项集成测试：`test_streaming_parser_matches_inmemory_for_examples`（一致性验证）、`test_postgres_streaming_load_matches_inmemory`（PostgreSQL 流式入库）、`test_postgres_auto_streaming_for_large_file`（环境变量 `GUT_LOADER_BIG_FILE_TEST=1` 开启的 100MB+ 自动调度连贯验证）
- 集成测试同时内置 employee 风格大文件生成器（LCG 伪随机 + `Compression::none`）以确保压缩后仍超阈值

## [1.1.0] - 2026-05-26

### 新增

- 完成 Task #13 Makefile 构建脚本：在项目根目录新增 `Makefile`，封装 npm 与 cargo 的常用操作为 5 类目标共 19 条命令
- 开发调试：`install` / `dev` / `dev-web` / `check` / `test` / `test-integration` / `lint`，其中 `test-integration` 自动拉起并清理 Docker MySQL/PostgreSQL 容器
- 构建打包：`build` / `build-macos` / `build-macos-arm64` / `build-macos-x64` / `build-windows` / `build-linux` / `build-all`，跨平台目标通过 `npm run tauri build -- --target <triple>` 透传 Tauri CLI 参数
- 数据库管理：`db-up` / `db-down` / `db-status`，统一管理本地 Docker 测试数据库（MySQL 3307、PostgreSQL 5433、密码 testpass123、库名 gut_test）
- 清理：`clean` / `clean-rust` / `rebuild`
- 帮助：`help` 设为 `.DEFAULT_GOAL`，使用 `@echo` 输出中文分类命令清单；所有目标声明为 `.PHONY`
- 同步更新 `docs/PRD.md` 新增「2.9 构建与开发命令」章节与技术要求表条目，更新 `docs/IMPLEMENTATION.md` 新增「5.5 Makefile 构建脚本」章节，并在项目结构树中加入 `Makefile`

## [1.0.0] - 2026-05-26

### 文档

- 完成 Task #11 项目文档完善：重写 `docs/PRD.md` / `docs/IMPLEMENTATION.md` / `README.md`，按功能维度（而非任务维度）组织内容，确保 PRD 与实现文档严格一一对应
- PRD 拆分为「项目概述 / 功能说明 / 技术要求 / 限制与约束 / 使用注意事项」5 个一级章节，覆盖文件解析、目录扫描、前置检查、多数据库支持、LLM 智能识别、数据加载、报告生成、桌面 GUI 8 类功能
- 实现文档拆分为「技术架构 / 模块说明 / 数据结构设计 / 难点解决方案 / 配置示例 / 项目结构 / 与 PRD 对应关系」7 个一级章节，新增系统架构与数据流两张主 Mermaid 图、UTF-8 多字节字段定位 / PostgreSQL 类型转换 / 并发加载与进度同步 / 数据库参数上限 / 跨平台磁盘空间获取 / LLM 响应降级 / 数据库密码编码 7 项难点解决方案
- README 重写为开源社区最佳实践版本：项目介绍、亮点、技术栈、快速开始、配置指南、使用流程、支持的数据库列表、项目结构、开发指南、FAQ
- 历史 changelog 整理：原本同时存在两条 `[0.2.0]`，将 LLM 集成模块的变更归并到 `[0.2.1]` 以恢复版本号单调递增

## [0.7.1] - 2026-05-26

### 修复

- 修复 PostgreSQL 批量插入对 INT/DECIMAL 列以纯字符串绑定时类型不匹配的问题：在生成的占位符上追加显式类型转换后缀（VARCHAR -> ::TEXT、DECIMAL -> ::NUMERIC、INT -> ::BIGINT），并对数值列的空字符串绑定为 NULL，避免 NUMERIC/BIGINT 转换失败
- `src-tauri/src/database/postgres.rs` 中 `batch_insert` 引入 `pg_cast_suffix` 辅助函数与 `ColumnType` 模式匹配，行为对调用方完全透明

### 新增

- 新增端到端集成测试 `src-tauri/tests/integration_test.rs`，覆盖：目录扫描配对 5 组文件、FLG 字段位置连续性校验、DAT 中文字段解析、前置检查全部通过、MySQL 单表完整加载与行数校验、PostgreSQL 全量 5 张表（12300 行）加载与报告生成校验、`ReportGenerator` 聚合统计正确性
- 集成测试在缺失 Docker 数据库时通过 `eprintln!` 打印跳过原因后正常返回，便于在无外部依赖时仍能运行其余用例

## [0.7.0] - 2026-05-26

### 新增

- 实现 Task #7 Tauri 命令层与 Task #9 前后端集成
- `src-tauri/src/commands/mod.rs` 新增 `AppState`（last_report / is_loading / cancel_flag 三组 `Arc<Mutex<>>`）与 9 条 Tauri 命令：`scan_directory`、`run_pre_checks`、`test_connection`、`start_loading`、`stop_loading`、`parse_db_info`、`test_llm_connection`、`get_report`、`save_report`
- `start_loading` 命令立即返回，真正加载在 tokio 后台任务中执行；通过 Tauri Emitter 推送 4 类事件：`loading-progress`（LoadProgress 单表实时进度）、`table-completed`（TableReport 单表完成）、`loading-completed`（LoadReport 整体完成）、`loading-error`（错误字符串）
- `stop_loading` 通过 `cancel_flag` 实现协作式取消，会在当前表完成后跳出循环
- `save_report` 把内存中的最近报告以 pretty JSON 持久化到指定路径并返回 JSON 文本
- `src-tauri/src/lib.rs` 注册 `AppState` 与命令处理器，保留 `tauri-plugin-dialog/shell/fs` 插件与开发态 devtools 启用
- 前端新增 `src/hooks/useLoadingEvents.ts` 钩子：在根组件挂载时一次性订阅 4 类后端事件并同步到 zustand store / sonner toast，组件卸载自动解绑
- `src/pages/HomePage.tsx` 调用 `useLoadingEvents` 完成全局事件订阅；`src/hooks/useTauriCommands.ts` 增补 `saveReport` 命令封装；`src/lib/types.ts` 中 `LoadStatus` 扩展 `completed_with_errors` 以匹配后端实际状态

### 变更

- 移除 `src-tauri/src/llm/parser.rs` 中本地重复定义的 `LlmConfig` 与 `DatabaseConfig`，统一改为 `use crate::models::{DatabaseConfig, LlmConfig}`，消除并行开发期间的类型分裂

## [0.6.0] - 2026-05-26

### 新增

- 实现 Task #6 报告生成模块（`src-tauri/src/report/mod.rs`）
- `ReportGenerator` 在加载过程中收集每张表的 `TableReport`，最终汇总为 `LoadReport`：包含总表数、总行数、成功/失败行数、成功率、总耗时与平均速率
- `export_json` 将报告以 pretty JSON 写入指定路径，自动创建缺失的父级目录
- `export_summary` 输出格式化文本摘要：汇总统计、各表详情表（含表名/总行数/成功/失败/耗时/速率）、错误详情（每表最多展示前 10 条并截断提示）
- `load_report_from_file` 提供从 JSON 文件回读 `LoadReport` 的能力，便于历史报告查看
- `format_duration` 将毫秒数格式化为 `Xms` / `X.Ys` / `XmY.Zs` 三档可读字符串
- 单元测试 12 项覆盖汇总聚合、空报告、成功率全成功/全失败、耗时格式化、文本摘要关键段落、错误截断、JSON 导出与回读、自动建目录、默认实例与缺失文件处理，全部通过

## [0.5.0] - 2026-05-26

### 新增

- 实现 Task #4 多数据库连接与批量写入模块
- `database/mod.rs` 定义 `DatabaseLoader` 统一 trait 接口（test_connection / create_table / batch_insert / get_row_count / close）及 `create_loader` 工厂函数
- `database/mysql.rs` 完整实现 MySQL/TXSQL/TDSQL 连接池管理、自动建表、批量插入（基于 sqlx MySqlConnectOptions）
- `database/postgres.rs` 完整实现 PostgreSQL/OpenGauss/GaussDB 连接池管理、schema 支持、自动建表、批量插入（基于 sqlx PgConnectOptions）
- `database/oracle.rs` 和 `database/dm.rs` Oracle/达梦适配器框架
- `loader/batch.rs` 实现 `load_table` 数据加载编排：解析 FLG -> 建表 -> 断点续传 -> 分批插入 -> 进度通知 -> 报告返回
- 添加 `safe_batch_size` 函数动态计算安全批次大小以防止超过数据库参数数量限制
- 添加 `async-trait = "0.1"` 依赖到 Cargo.toml

## [0.4.0] - 2026-05-26

### 新增

- 实现 Task #3 前置检查与验证模块（`src-tauri/src/validator/`）
- `validator::run_all_checks` 统一入口，一次性执行所有前置检查并返回 `Vec<PreCheckResult>`
- `validator::disk` 磁盘空间检查：源目录可用空间 >= 数据文件总大小 × 2，目标路径可用空间 >= 1GB
- `validator::file` 文件格式验证：目录存在性、文件扫描、命名格式、FLG 可解析性、DAT 存在性、字段数量匹配、字段位置连续性、GZIP 完整性、行数匹配
- 单元测试 18 项全部通过，覆盖 example_data 下 5 组文件对的完整检查流程

## [0.3.0] - 2026-05-26

### 新增

- 实现 Task #8 桌面前端 UI：以 5 步向导承载完整入库流程（选择源文件 / 配置目标库 / 前置检查 / 执行加载 / 查看报告）
- 状态层 `src/stores/appStore.ts`：zustand 全局 store，覆盖向导状态、文件对、数据库配置、LLM 配置、预检结果、加载进度、实时日志、报告与重置
- 类型层 `src/lib/types.ts`：对齐后端 `models`，同时导出 `DB_TYPE_LABEL` 与 `DB_TYPE_DEFAULT_PORT`
- IPC 封装 `src/hooks/useTauriCommands.ts`：8 个命令 `scan_directory / run_pre_checks / test_connection / start_loading / stop_loading / parse_db_info / test_llm_connection / get_report`，后端未就绪时返回兜底值
- 页面层 `src/pages/HomePage.tsx`：sticky Header、可点击跳转的 5 列 Stepper、主面板与底部导航，`useMemo` 派发 `canAdvance` 逻辑
- 组件层：`FileSelector` / `DatabaseConfig` / `LLMConfig` / `PreCheckPanel` / `LoadingProgress` / `ReportView`；报告页采用 recharts BarChart + PieChart 并支持 JSON 导出
- 设计系统：index.html 引入 Inter Tight + JetBrains Mono Webfont；index.css 重定义主题变量（新增 amber `--accent`）、`bg-grid-pattern` 背景以及默认 tabular-nums
- 加载页：LIVE 指示灯、总体/表级进度、终端风格实时日志（原生 div + overflow-y-auto 自动滚动到底）
- 报告页：汇总四联卡（成功率色带分级）、总耗时大数、每表详情表、柱状与饼图、JSON 导出

### 变更

- 重写 `src/App.tsx`：移除残留路由页，直接渲染 `HomePage`、使用带 closeButton 的 sonner Toaster
- 主题重调：`src/index.css` 重设双主题颜色变量，加入信号色与网格背景工具类

## [0.2.1] - 2026-05-26

### 新增

- 实现 Task #5 LLM 集成模块（`src-tauri/src/llm/parser.rs`）：基于 OpenAI 兼容 Chat Completions 协议的 `LlmClient`，支持自定义 base URL（OpenAI / DeepSeek / 通义千问等）
- 提供 `parse_database_info` 方法：将自然语言文本解析为 `ParsedDbInfo`，包含 db_type / host / port / database / username / password / schema 与 confidence 字段
- 设计结构化中文系统提示词，约束模型仅返回 JSON；启用 `response_format=json_object` 与 `temperature=0.0` 提升稳定性
- 提供 `validate_config` 方法：发送 `Hello` 探活请求验证 API 配置可用性
- 提供 `ParsedDbInfo::to_database_config` 转换方法，缺失字段以默认值（mysql / localhost / 3306）补齐
- 健壮的响应处理：直接 JSON 反序列化失败时回退至花括号配平提取首个 JSON 对象再解析，仍失败则降级为空结果
- HTTP 客户端默认超时 30 秒，认证错误（401/403）单独识别并返回详细错误
- 使用 `tracing` 记录请求、响应与错误信息
- 单元测试覆盖纯 JSON 解析、Markdown 围栏、嵌入式 JSON、嵌套对象、部分字段降级、配置校验与端点拼接等场景，13 项测试全部通过

## [0.2.0] - 2026-05-26

### 新增

- 填充 `src-tauri/src/models.rs` 跨模块数据结构：`ColumnType`、`ColumnDefinition`、`FlgMetadata`、`DataRow`、`GutFilePair`、`DatabaseConfig`、`LlmConfig`、`LoadProgress`、`LoadReport`、`TableReport`、`PreCheckResult`，统一派生 `serde::{Serialize, Deserialize}`
- 实现 `parser::flg::parse_flg`：解析 .flg 文件里的重点键值对与 `COLUMNDECRIPTION` 字段定义，支持 VARCHAR / DECIMAL / INT 与半/全角括号
- 实现 `parser::dat::parse_dat` 与 `parse_dat_streaming`：基于 `flate2::GzDecoder` 流式读取 `.dat.gz`，按字节偏移提取字段以适配 UTF-8 多字节场景，并提供大文件分批回调 API
- 实现 `parser::scan_directory`：按 `<TABLE>.<DATE>.<TIME>.<SEQ>.<EXT>` 命名规则在目录内配对 .flg 与 .dat.gz，输出有序 `GutFilePair` 列表
- 增加解析器单元测试（FLG / DAT / 目录扫描 / 中文字段 / 流式分批），`cargo test --lib` 全部通过

## [0.1.0] - 2026-05-26

### 新增

- 初始化 Tauri v2 + React 18 + TypeScript + Vite 6 桌面工程骨架
- 配置 Tailwind CSS 3 与 shadcn/ui（new-york 风格、slate 基础色）主题体系，含 light/dark 双主题 CSS 变量
- 引入前端依赖：react-router-dom、zustand、recharts、lucide-react、sonner、@tauri-apps/api 等
- 提供 shadcn/ui 基础组件：button、card、input、label、select、progress、tabs、badge、dialog、separator、scroll-area、sonner
- 建立 Rust 后端骨架（`src-tauri`）：parser、database、validator、llm、loader、report、commands、models 模块
- 配置 Tauri 窗口（1280x800，标题「GUT数据加载工具」）与 capabilities 默认权限
- 添加项目文档 `docs/PRD.md`、`docs/IMPLEMENTATION.md`、`docs/changelog.md` 以及 `README.md`
