# GUT 数据加载工具 - 实现文档

本文档描述 GUT 数据加载工具的技术实现，与 `docs/PRD.md` 中的功能需求一一对应。文档结构按「技术架构 -> 模块说明 -> 数据结构 -> 难点解决方案 -> 配置与项目结构」自上而下展开。

## 1. 技术架构

### 1.1 技术选型

| 层 | 技术 | 版本 | 选型理由 |
| --- | --- | --- | --- |
| 桌面框架 | Tauri | 2.x | 体积小、安全模型完善，原生支持 macOS / Windows / Linux |
| 前端框架 | React | 18 | 生态成熟，配合 hooks 与 Suspense 易于状态拆分 |
| 前端语言 | TypeScript | 5.7 | 类型安全，前后端类型对齐 |
| 构建工具 | Vite | 6 | 极速 HMR、原生 ESM |
| UI 组件 | shadcn/ui + Radix UI | new-york 风格 | 可定制无锁定，符合现代审美 |
| 样式 | Tailwind CSS | 3.4 | 原子化 CSS，适配 shadcn 主题变量 |
| 状态管理 | Zustand | 5 | 轻量、无样板代码 |
| 图标 | lucide-react | 最新 | 与 shadcn 默认图标体系一致 |
| 图表 | recharts | 2.15 | React 友好的可视化方案 |
| 路由 | react-router-dom | 7 | 留作扩展占位，主流程使用单页向导 |
| 通知 | sonner | 1.7 | 简洁的 toast 实现 |
| 后端语言 | Rust | edition 2021 | 高性能、并发安全 |
| 异步运行时 | tokio | 1（features = full） | 标准异步生态 |
| 数据库驱动 | sqlx | 0.8（mysql + postgres） | 编译期 SQL 校验、原生异步、连接池 |
| 解压 | flate2 | 1 | gz 流式解压 |
| HTTP 客户端 | reqwest | 0.12（features = json） | LLM 调用 |
| 异步 trait | async-trait | 0.1 | 为 `DatabaseLoader` 提供异步方法支持 |
| 错误处理 | thiserror + anyhow | 2 / 1 | 库与应用层错误分层 |
| 日志 | tracing + tracing-subscriber | 0.1 / 0.3 | 结构化日志、target 过滤 |
| 时间 | chrono | 0.4（serde） | 时间戳序列化 |
| ID | uuid | 1（v4） | 任务 ID 生成 |

### 1.2 系统架构图

```mermaid
graph TB
    subgraph Frontend [前端 React + TypeScript]
        UI[页面与组件]
        Store[Zustand Store]
        Bridge[Tauri invoke / event]
    end

    subgraph Backend [后端 Rust + Tauri]
        Cmd[commands]
        Parser[parser]
        Validator[validator]
        Loader[loader]
        DB[database]
        LLM[llm]
        Report[report]
        Models[models]
    end

    UI --> Store
    Store --> Bridge
    Bridge --> Cmd
    Cmd --> Parser
    Cmd --> Validator
    Cmd --> Loader
    Cmd --> LLM
    Cmd --> Report
    Loader --> DB
    Parser --> Models
    Loader --> Models
```

说明：

- 前端的所有 IPC 调用集中封装在 `useTauriCommands` 钩子中，事件订阅集中在 `useLoadingEvents` 钩子中，组件层只与 Zustand store 交互。
- 后端的 `commands` 模块作为唯一与 UI 通信的边界，下游模块（parser / validator / loader / database / llm / report）彼此解耦，仅依赖 `models` 模块的共享数据结构。
- 商业数据库实现中 Oracle 基于 oracle-rs 0.1（纯 Rust TNS 协议）默认参与编译，零系统依赖运行（无需 Oracle Instant Client / OCI / ODPI-C）；达梦 DM 基于 odbc-api 8 默认参与编译，驱动二进制随 Tauri 资源打包随安装包分发，运行时通过可执行文件相对路径动态发现。

### 1.3 数据流图

```mermaid
graph TB
    Dir[源目录] --> Scan[scan_directory]
    Scan --> Pairs[GutFilePair 列表]
    Pairs --> Pre[run_all_checks]
    Pre --> PreResults[PreCheckResult 列表]
    Pairs --> Loader[load_table 逐表加载]
    Loader --> ParseFlg[parse_flg]
    ParseFlg --> Meta[FlgMetadata]
    Loader --> ParseDat[parse_dat_streaming]
    Meta --> ParseDat
    ParseDat --> Batches[DataRow 批次]
    Loader --> CreateTable[DatabaseLoader.create_table]
    Loader --> Insert[DatabaseLoader.batch_insert]
    Insert --> Progress[mpsc 进度通道]
    Progress --> Emit[Tauri emit loading-progress]
    Loader --> TableReport
    TableReport --> Generator[ReportGenerator]
    Generator --> LoadReport
    LoadReport --> EmitFinal[emit loading-completed]
    LoadReport --> ExportJson[export_json]
```

说明：

- 数据从目录扫描进入，经过解析层产出元数据与数据行，再由 `loader` 协调 `database` 适配器完成写入；进度沿 mpsc 通道反向流向 `commands` 层 emit 至前端。
- `ReportGenerator` 仅消费每张表的 `TableReport`，不关心数据来源，因此可以独立用于离线汇总或回看场景。

## 2. 模块说明

### 2.1 文件解析模块

路径：`src-tauri/src/parser/`

| 子模块 | 职责 |
| --- | --- |
| `flg.rs` | 解析 `.flg` 元数据：表名、文件大小、行数、SQL、字节长度、字段定义列表 |
| `dat.rs` | 解析 `.dat` / `.dat.gz` 数据文件：按字节偏移提取字段、流式分批回调 |
| `mod.rs` | 暴露 `scan_directory` 函数，按命名前缀配对 `.flg` 与 `.dat.gz` |

实现要点：

- `parse_flg` 首行作为摘要跳过；后续行依 `KEY=VALUE` 解析关键键值对；`COLUMNDECRIPTION=` 之后进入字段定义模式，按 `$$` 切分；数据类型解析采用括号定位 + uppercase 匹配，兼容半角与全角括号、半角与全角逗号
- `parse_dat` / `parse_dat_streaming` 内部使用 `BufReader::with_capacity(64KB)` 包裹 `GzDecoder`（按扩展名判别），`read_until(b'\n')` 逐行读取并去除尾部 `\r\n`；字段提取严格基于 `&[u8]` 字节切片，再以 `std::str::from_utf8` 转换为 `&str`
- `scan_directory` 使用 `HashMap<前缀, Slot>` 对 `.flg` 与 `.dat.gz` 进行合并配对，仅当双边都存在时输出 `GutFilePair`，孤立文件记入警告日志
- 异常输入采用「记录日志 + 跳过」策略，最大化保留可用数据

### 2.2 前置检查模块

路径：`src-tauri/src/validator/`

| 子模块 | 职责 |
| --- | --- |
| `disk.rs` | 通过 `df -k` 命令获取可用磁盘空间，校验源目录与目标路径阈值 |
| `file.rs` | 文件命名格式、FLG 可解析性、字段位置连续性、GZIP 完整性、行数匹配等多项检查 |
| `mod.rs` | 暴露 `run_all_checks(directory, target_path)` 统一入口 |

实现要点：

- 跨平台磁盘空间获取通过 `std::process::Command` 调用 `df -k` 解析输出第二行第 4 列（Available），无需引入 libc
- 路径不存在时回退至父目录执行 `df`，避免命令直接失败
- GZIP 完整性仅解压前 5 行验证可读性；行数匹配则流式解压全部内容计数，两者独立报告
- 字段位置连续性按 `start_pos` 排序后逐对验证 `prev.end_pos + 1 == next.start_pos`，并校验首字段起 1、末字段终 `ROWLENGTH`

```mermaid
graph TB
    Entry[run_all_checks] --> Disk[disk::check_disk_space]
    Disk --> TargetCheck{target_path 是否提供}
    TargetCheck -- 是 --> TargetDisk[disk::check_target_disk_space]
    TargetCheck -- 否 --> Files
    TargetDisk --> Files[file::check_all_files]
    Files --> DirExists[目录存在性]
    DirExists --> Scan[parser::scan_directory]
    Scan --> Loop[遍历 GutFilePair]
    Loop --> Naming[命名格式]
    Loop --> Flg[FLG 可解析]
    Loop --> Dat[DAT 文件存在]
    Loop --> Cols[字段数量与位置]
    Loop --> Gz[GZIP 完整性]
    Loop --> Rows[行数匹配]
    Loop --> Out[PreCheckResult 列表]
```

说明：所有检查项独立返回 `PreCheckResult`，单项失败不影响其他项执行（FLG 解析失败除外，因其为后续字段位置 / 行数检查的前提）。

### 2.3 数据库模块

路径：`src-tauri/src/database/`

| 子模块 | 职责 |
| --- | --- |
| `mod.rs` | 定义 `DatabaseLoader` trait 与 `create_loader` 工厂函数；`column_type_to_*_ddl` / `safe_batch_size` 等辅助函数 |
| `mysql.rs` | MySQL / TXSQL / TDSQL 适配器（基于 sqlx::MySqlPool） |
| `postgres.rs` | PostgreSQL / openGauss / GaussDB 适配器（基于 sqlx::PgPool） |
| `oracle.rs` | Oracle 适配器（默认参与编译，基于 oracle-rs 0.1 纯 Rust TNS 协议驱动） |
| `dm.rs` | 达梦 DM 适配器（默认启用，基于 odbc-api/ODBC）；含 `resolve_driver_path` 运行时发现打包驱动 |

`DatabaseLoader` 接口：

| 方法 | 签名 | 说明 |
| --- | --- | --- |
| `test_connection` | `async fn(&self) -> Result<bool>` | 探活查询 |
| `create_table` | `async fn(&self, meta: &FlgMetadata) -> Result<()>` | `CREATE TABLE IF NOT EXISTS` |
| `batch_insert` | `async fn(&self, table: &str, columns: &[ColumnDefinition], rows: &[DataRow]) -> Result<usize>` | 多行批量插入 |
| `get_row_count` | `async fn(&self, table: &str) -> Result<i64>` | 表内行数（断点续传） |
| `close` | `async fn(self: Box<Self>) -> Result<()>` | 关闭连接池 |

工厂路由（`db_type` 大小写不敏感）：

| `db_type` | 适配器 |
| --- | --- |
| `mysql` / `txsql` / `tdsql` | `MysqlLoader` |
| `postgresql` / `postgres` / `opengauss` / `gaussdb` | `PostgresLoader` |
| `oracle` | `OracleLoader`（默认参与编译） |
| `dameng` | `DmLoader`（默认启用） |

实现要点：

- 使用 `MySqlConnectOptions` / `PgConnectOptions` 结构体构造连接，避免 URL 编码问题（密码含特殊字符）
- 连接池配置：最大 10 连接、最小 1 连接、30 秒获取超时
- DDL 类型映射：`Varchar(n)` -> `VARCHAR(n)`、`Decimal(m,n)` -> `DECIMAL/NUMERIC(m,n)`、`Int(n)` -> `BIGINT`
- MySQL 占位符使用 `?`，PostgreSQL 使用编号 `$N` 并附加 `::TEXT/::NUMERIC/::BIGINT` 显式类型转换后缀；PG 数值列空字符串绑定为 `NULL`
- `safe_batch_size(column_count, desired)` 以 60000 参数上限为基准动态收缩批次行数，确保 `字段数 × 行数 ≤ 60000`
- Oracle 适配器基于 oracle-rs 0.1（纯 Rust TNS 协议驱动）实现完整的连接、建表、批量插入、行数查询能力。选择 oracle-rs 的理由：它以纯 Rust 实现 Oracle TNS 协议，不依赖 Oracle Instant Client / OCI / ODPI-C 等任何系统库，使最终二进制可在零 Oracle 客户端环境下运行。连接管理：使用 [`oracle_rs::Config::new(host, port, service, user, password)`] + [`oracle_rs::Connection::connect_with_config`] 建立异步会话；Connection 内部以互斥锁串行化协议交互，可在 `&self` 上并发调用。可选 `schema` 通过 `ALTER SESSION SET CURRENT_SCHEMA` 生效并作为表名限定前缀。DDL 策略：建表前查询 `user_tables`（或有 schema 时查 `all_tables`）数据字典判断表是否已存在，已存在则跳过；类型映射 Varchar(n) -> `VARCHAR2(n)`、Decimal(m,n) -> `NUMBER(m,n)`、Int(n) -> `NUMBER(19)`；表名与列名使用双引号转义。批量插入：采用 [`oracle_rs::BatchBuilder::add_row`] 逐行累加绑定参数，单次 [`oracle_rs::Connection::execute_batch`] 提交一个分片；数值列占位符使用 `TO_NUMBER(NULLIF(:n, ''))` 包装以将空串安全转为 NULL；单个分片失败仅记录警告日志不中断后续批次，完成后调用 `Connection::commit` 统一提交。关闭路径调用 `Connection::close` 主动发送 logoff 报文并断开 TCP 连接
- 达梦适配器仅在 Windows x86_64 / Linux x86_64 / Linux aarch64 上启用。选择 odbc-api 的理由：达梦未提供 Rust 原生驱动，ODBC 是其官方支持的通用接入协议，odbc-api 是 Rust 生态中最成熟的 ODBC 封装。驱动发现策略：`resolve_driver_path()` 在构造 `DmLoader` 时依次尝试可执行文件相对位置的资源目录与开发模式目录，目录层级固定为 `bundled-drivers/dm-odbc/<platform>/<arch>/`。macOS 不编译达梦适配器，也不提供入口；Windows 仅使用 `x64`，Linux 支持 `x64` 与 `arm64`。异步包装策略：由于 ODBC `Connection` 与 `Environment` 不是 Send + Sync，采用持有连接字符串、每次操作在 `tokio::task::spawn_blocking` 内创建独立 `Environment` 和 `Connection` 的模式。连接字符串格式为 `Driver={<驱动路径或名称>};Server=host;TCP_PORT=port;DATABASE=db;UID=user;PWD={password}`，密码中的 `}` 转义为 `}}`。DDL 策略：建表前查询 `user_tables` 数据字典判断表是否已存在；类型映射 Varchar(n) -> `VARCHAR(n)`、Decimal(m,n) -> `DECIMAL(m,n)`、Int(n) -> `BIGINT`；表名与列名使用双引号转义。批量插入：使用 prepared statement 逐行通过 `into_parameter()` 将字符串转换为 ODBC 可绑定参数执行，手动事务模式下分批提交，单行失败仅记录警告不中断后续批次。打包集成：`src-tauri/bundled-drivers/dm-odbc/{windows,linux}/{x64,arm64}` 三平台目录与 `odbcinst.ini` 模板随 Tauri `bundle.resources` 字段（`bundled-drivers/**/*`）一并打入安装包，驱动二进制（`*.so` / `*.dll`）不随仓库提交，由开发者从达梦官方安装介质中提取后放入对应平台目录

### 2.4 LLM 集成模块

路径：`src-tauri/src/llm/`

| 子模块 | 职责 |
| --- | --- |
| `parser.rs` | `LlmClient` 与自然语言解析能力的核心实现 |
| `mod.rs` | 重导出 `parser` 中的公开符号 |

实现要点：

- 基于 OpenAI Chat Completions 协议，端点拼接 `endpoint() = api_url.trim_end_matches('/') + "/chat/completions"`
- 请求体启用 `temperature = 0.0`、`response_format = json_object`（探活请求不携带 `response_format` 以兼容部分服务）
- 使用 `bearer_auth(api_key)` 携带凭证，默认 30 秒超时
- 响应解析降级：先 `serde_json::from_str::<ParsedDbInfo>` 直接反序列化整段文本；失败则调用 `extract_json_object` 提取首个花括号配平的 JSON 片段；仍失败则返回默认空结果
- `extract_json_object` 采用有限状态扫描：跟踪 `{}` 深度、`"` 字符串范围与 `\` 转义，正确处理嵌套对象与字符串内的 `}`
- 输入文本 trim 后为空时不发起请求，直接返回默认 `ParsedDbInfo`
- 401 / 403 状态码识别为认证失败错误，单独区分；其他非 2xx 返回 `LLM 接口异常 (HTTP xxx): body`
- `validate_config()` 校验 `api_url` / `api_key` / `model` 三项均不为空，再发送 `Hello` 探活请求

```mermaid
graph TB
    Input[自然语言文本] --> Empty{trim 后为空}
    Empty -- 是 --> Default[返回默认 ParsedDbInfo]
    Empty -- 否 --> Build[拼接 system + user 消息]
    Build --> Send[POST chat/completions]
    Send --> Resp{HTTP 状态}
    Resp -- 401/403 --> AuthErr[认证错误]
    Resp -- 其他非 2xx --> ApiErr[接口异常]
    Resp -- 2xx --> Content[提取 message.content]
    Content --> Direct{直接反序列化成功}
    Direct -- 是 --> Out[ParsedDbInfo]
    Direct -- 否 --> Extract[抽取首个 JSON 片段]
    Extract --> Retry{反序列化成功}
    Retry -- 是 --> Out
    Retry -- 否 --> EmptyOut[返回默认 ParsedDbInfo]
```

说明：流程图覆盖入口护栏、HTTP 错误分级与响应解析降级三道关键路径，确保前端在任何输入下都能拿到可用的 `DatabaseConfig`。

### 2.5 数据加载模块

路径：`src-tauri/src/loader/`

| 子模块 | 职责 |
| --- | --- |
| `batch.rs` | `load_table` 入口按文件大小调度，包含 `load_table_inmemory`（内存一次加载）与 `load_table_streaming`（流式分片入库）两条路径 |
| `mod.rs` | 重导出 `load_table` / `load_table_inmemory` / `load_table_streaming` / `STREAMING_THRESHOLD_BYTES` |

`load_table` 调度与两条路径的完整流程：

```mermaid
graph TB
    Start[load_table 入口] --> SizeCheck{dat.gz 大小 > 100MB ?}
    SizeCheck -->|是| StreamPath[load_table_streaming]
    SizeCheck -->|否| InmemPath[load_table_inmemory]

    StreamPath --> ParseFlgS[parse_flg]
    ParseFlgS --> CreateTableS[create_table]
    CreateTableS --> ExistingS[get_row_count]
    ExistingS --> ReadLine[BufReader.read_until 逐行读取]
    ReadLine --> SkipS{行号 <= existing?}
    SkipS -->|是| ReadLine
    SkipS -->|否| ParseRow[parse_row_bytes]
    ParseRow --> Buf[追加入本地 batch]
    Buf --> FullCheck{batch 达 batch_size?}
    FullCheck -->|是| Insert[batch_insert + 清空]
    FullCheck -->|否| ReadLine
    Insert --> Progress[mpsc 推送 LoadProgress]
    Progress --> ReadLine
    ReadLine --> Tail[末尾剩余数据 flush]
    Tail --> Final[聚合 TableReport]

    InmemPath --> ParseFlgI[parse_flg]
    ParseFlgI --> CreateTableI[create_table]
    CreateTableI --> ExistingI[get_row_count]
    ExistingI --> ParseAll[parse_dat 全量加载]
    ParseAll --> SkipI[切片跳过已加载行]
    SkipI --> BatchLoop[chunks(batch_size) 循环]
    BatchLoop --> InsertI[batch_insert]
    InsertI --> ProgressI[mpsc 推送 LoadProgress]
    ProgressI --> BatchLoop
    BatchLoop --> FinalI[聚合 TableReport]
```

实现要点：

- 阈值 `STREAMING_THRESHOLD_BYTES = 100 * 1024 * 1024` 以公开常量导出供下游与集成测试引用
- 默认批次大小 1000 行，由 `safe_batch_size` 根据字段数自适应缩减（`字段数 × 行数 ≤ 60000`）
- 流式路径复用 `parser::dat::parse_row_bytes` 与 `parser::dat::is_gzip`（已提升为 `pub(crate)`）避免重复实现行解析逻辑
- 流式路径使用 `BufReader::with_capacity(64 KB)` 包裹 `GzDecoder`，`read_until(b'\n')` 逐行读取后去除 `\r\n`，空行跳过
- 进度推送使用 `tokio::sync::mpsc::Sender::try_send`，避免前端消费不及时阻塞加载
- 单批插入失败仅记录错误并写入 `TableReport.errors`，不中断后续批次；单行解析失败仅以 `warn!` 记录，继续后续行
- 速度计算 `success_count * 1000 / elapsed_ms`，零耗时返回 0
- 两条路径都以 `info!` 记录走了哪个分支以便于运维诊断

### 2.6 报告生成模块

路径：`src-tauri/src/report/mod.rs`

`ReportGenerator` 状态：

- `table_reports: Vec<TableReport>`：累积单表结果
- `start_time: std::time::Instant`：报告起算点

聚合指标在 `generate()` 中实时计算：

- `total_rows = Σ row_count`
- `success_rows = Σ success_count`
- `failed_rows = Σ failed_count`
- `success_rate = success_rows / total_rows`（0 总行数返回 0）
- `total_elapsed_ms = start_time.elapsed().as_millis() as u64`
- `avg_speed = success_rows / (total_elapsed_ms / 1000)`（零耗时返回 0）

```mermaid
graph TB
    Loader[loader::load_table] -->|TableReport| Generator[ReportGenerator]
    Generator --> Generate[generate]
    Generate --> Report[LoadReport]
    Generator --> ExportJson[export_json]
    Generator --> ExportSummary[export_summary]
    ExportJson --> JsonFile[reports/*.json]
    JsonFile --> Reload[load_report_from_file]
    Reload --> Report
```

说明：批量加载器在每张表完成后向 `ReportGenerator` 投递 `TableReport`；最终通过 `generate` 得到结构化报告，可通过 `export_json` 落盘并由 `load_report_from_file` 在历史查看时复原。

实现要点：

- `export_json` 写入前对父目录执行 `create_dir_all` 兜底，避免 `NotFound`
- `export_summary` 使用 `format!` 拼接固定宽度的列；错误详情按表分组、单表最多 10 条、超出截断
- `format_duration` 提供毫秒/秒/分钟三档可读字符串

### 2.7 Tauri 命令层

路径：`src-tauri/src/commands/mod.rs`、`src-tauri/src/lib.rs`

`AppState` 是跨命令共享的运行时状态：

- `last_report: Arc<Mutex<Option<LoadReport>>>`：最近一次成功生成的报告
- `is_loading: Arc<Mutex<bool>>`：加载中标志，重复启动会被拒绝
- `cancel_flag: Arc<Mutex<bool>>`：协作式取消标志

通过 `tauri::Builder::manage(AppState::new())` 注册，所有命令通过 `tauri::generate_handler!` 暴露给前端。

命令清单：

| 命令 | 入参 | 返回 | 说明 |
| --- | --- | --- | --- |
| `scan_directory` | `path: String` | `Vec<GutFilePair>` | 代理 `parser::scan_directory` |
| `run_pre_checks` | `path, db_config` | `Vec<PreCheckResult>` | 代理 `validator::run_all_checks` |
| `test_connection` | `config: DatabaseConfig` | `bool` | 调用 `create_loader` 后 `test_connection` |
| `parse_db_info` | `text, llm_config` | `DatabaseConfig` | LLM 解析自然语言连接信息 |
| `test_llm_connection` | `config: LlmConfig` | `bool` | 代理 `LlmClient::validate_config` |
| `start_loading` | `path, db_config` | `()` | 启动后台加载任务并立即返回 |
| `stop_loading` | — | `()` | 设置 `cancel_flag = true` |
| `get_report` | — | `Option<LoadReport>` | 读取 `last_report` |
| `save_report` | `path: String` | `String` | pretty JSON 写入磁盘并返回 JSON 文本 |

事件推送：`start_loading` 立即返回，加载在 `tokio::spawn` 后台任务中执行，过程中 `app.emit` 推送 4 类事件：

```mermaid
graph TB
    Front[前端 invoke start_loading] --> Cmd[commands::start_loading]
    Cmd --> Spawn[tokio::spawn run_loading_task]
    Spawn --> Scan[parser::scan_directory]
    Scan --> Build[create_loader]
    Build --> Loop[逐表 load_table]
    Loop -->|mpsc| Forward[转发协程]
    Forward -->|emit loading-progress| Front
    Loop -->|emit table-completed| Front
    Loop --> Done[ReportGenerator.generate]
    Done -->|emit loading-completed| Front
    Loop -->|emit loading-error| Front
```

说明：每张表加载时会创建独立的 mpsc 通道与转发协程，转发协程在 `Sender` drop 后自然退出，外层 `await` 确保它在下一表开始前完成，事件按表隔离不串插。

### 2.8 前端 UI

路径：`frontend/src/`

| 子目录 / 文件 | 职责 |
| --- | --- |
| `pages/HomePage.tsx` | 5 步向导容器、Sticky Header、Stepper、底部导航、`canAdvance` 派发 |
| `components/FileSelector.tsx` | 步骤 01：目录选择与扫描结果展示 |
| `components/DatabaseConfig.tsx` | 步骤 02：数据库参数表单与连接测试 |
| `components/LLMConfig.tsx` | 步骤 02：LLM 服务配置表单，由 `DatabaseConfig.tsx` 在 Dialog 弹窗中渲染 |
| `components/PreCheckPanel.tsx` | 步骤 03：前置检查执行与按 severity 渲染 |
| `components/LoadingProgress.tsx` | 步骤 04：总进度 / 表级进度 / 终端日志面板 |
| `components/ReportView.tsx` | 步骤 05：报告四联卡 / 详情表 / 柱图 / 饼图 / JSON 导出 |
| `components/ui/*` | shadcn/ui 基础组件 |
| `stores/appStore.ts` | Zustand 全局状态：向导步骤、配置、预检结果、进度、日志、报告、reset |
| `hooks/useTauriCommands.ts` | IPC 封装层，集中 try/catch 与兜底返回值 |
| `hooks/useLoadingEvents.ts` | 事件订阅层，挂载根组件时一次性订阅 4 类事件 |
| `lib/types.ts` | 与后端 `models.rs` 一致的 TypeScript 类型 |
| `lib/utils.ts` | `cn` 工具与通用辅助函数 |

实现要点：

- 状态层使用 `useAppStore` 单一全局 store；`appendLoadingLog` 限制日志缓存为 500 行避免内存压力；LLM 配置通过 localStorage 持久化（key 为 `gut-loader-llm-config`），应用启动时自动加载，变更时自动保存；数据库配置通过 localStorage 持久化（key 为 `gut-loader-saved-db-configs`），支持保存、加载、删除已命名的配置，保存时密码字段自动置空
- `isStepCompleted` 回调函数（位于 `HomePage.tsx`）：以 `useCallback` 封装，根据传入的步骤索引返回布尔值。内部通过 switch-case 分发，每个步骤的完成条件与 `canAdvance` 逻辑类似但匹配前一步的验证语义：步骤 0 检查文件对是否非空、步骤 1 检查数据库必填项、步骤 2 检查预检无 error 级失败、步骤 3 检查报告是否已生成、步骤 4 始终返回 false。Stepper 组件以此回调的返回值决定是否渲染绿色对勾图标
- `PreCheckPanel` 自动触发机制：组件内部使用 `useRef(false)` 创建 `hasAutoRun` 标志，并在 `useEffect([], ...)` 中检查当 `selectedDirectory` 非空且 `hasAutoRun.current` 为 false 时立即调用 `handleRun()` 并将标志置为 true，确保用户进入步骤 3 时自动开始前置检查，同时避免因组件重渲染导致重复触发
- LLM 配置默认值为空字符串，输入框以 placeholder 显示示例值；智能识别面板默认展开，`<LLMConfig />` 由面板标题栏「配置 LLM」按钮触发的 Dialog 弹窗承载，未配置时按钮右上角显示 amber 色小圆点状态指示；LLM 未配置时智能识别按钮禁用并显示提示
- 类型层 `DB_TYPE_LABEL`、`DB_TYPE_DEFAULT_PORT` 在切换数据库类型时驱动默认端口填充
- IPC 封装统一捕获异常并返回兜底值（空数组 / null / false），辅以 toast 提示，使 UI 在后端不可用时仍可走通
- 加载页 LIVE 指示灯仅在 `isLoading=true` 时以 ping 动画闪烁，日志面板使用原生 `<div>` + `overflow-y-auto` 实现自动滚动到底
- 报告页使用 recharts BarChart + PieChart，颜色与 `--accent` / emerald / destructive 主题对齐；导出使用 `Blob` + `URL.createObjectURL`
- 跨平台视觉一致性（`index.css`）：全局 `*` 选择器统一应用 `scrollbar-width: thin` 与 `scrollbar-color`（Firefox）、`-webkit-tap-highlight-color: transparent`；`body` 启用 `-webkit-font-smoothing: antialiased` 与 `-moz-osx-font-smoothing: grayscale` + `text-rendering: optimizeLegibility` 实现字体渲染统一；Webkit 滚动条通过 `::-webkit-scrollbar` / `::-webkit-scrollbar-track` / `::-webkit-scrollbar-thumb` 三级自定义（宽 8px、圆角 4px、主题边框色）；`::selection` 统一使用 accent 色 30% 透明度作为选中背景；`@media (forced-colors: active)` 媒体查询为高对比度模式下的 `focus-visible` 提供 2px 实线轮廓

```mermaid
graph LR
    A[01 选择源文件] -->|filePairs.length > 0| B[02 配置目标库]
    B -->|host port database username 均有值| C[03 前置检查]
    C -->|无 severity=error 项| D[04 执行加载]
    D -->|isLoading=false 或已有 report| E[05 查看报告]
    E -->|reset| A
```

说明：`HomePage` 内部以 `useMemo` 实时计算 `canAdvance`，为 `false` 时禁用「下一步」按钮，保证状态联动一致。

## 3. 数据结构设计

所有跨模块结构定义在 `src-tauri/src/models.rs`，统一派生 `serde::{Serialize, Deserialize}` 以便前后端互通。前端在 `frontend/src/lib/types.ts` 中保持字段名与类型完全一致。

### 3.1 解析层结构

```json
{
  "ColumnType": ["Varchar(n)", "Decimal(m,n)", "Int(n)"],
  "ColumnDefinition": {
    "index": 1,
    "name": "EMP_NO",
    "data_type": "Varchar(20)",
    "start_pos": 1,
    "end_pos": 20
  },
  "FlgMetadata": {
    "filename": "employee.20260421.000000.0000.dat.gz",
    "file_size": 10,
    "row_count": 800,
    "created_at": "2026-04-23 00:06:58",
    "sql": "SELECT * FROM t_employee",
    "row_length": 212,
    "column_count": 6,
    "columns": ["...ColumnDefinition..."],
    "table_name": "employee"
  },
  "DataRow": { "values": ["U0000000000000000001", "张伟", "..."] },
  "GutFilePair": {
    "table_name": "employee",
    "date": "20260421",
    "time": "000000",
    "sequence": "0000",
    "flg_path": "./example_data/employee.20260421.000000.0000.flg",
    "dat_path": "./example_data/employee.20260421.000000.0000.dat.gz"
  }
}
```

### 3.2 配置与运行时结构

```json
{
  "DatabaseConfig": {
    "db_type": "mysql|postgresql|opengauss|txsql|tdsql|gaussdb|oracle|dameng",
    "host": "127.0.0.1",
    "port": 3306,
    "database": "ods",
    "username": "loader",
    "password": "***",
    "schema": null
  },
  "SavedDbConfig": {
    "id": "m1a2b3c",
    "name": "生产库-PostgreSQL",
    "db_type": "postgresql",
    "host": "10.20.30.40",
    "port": 5432,
    "database": "ods",
    "username": "loader",
    "password": "",
    "schema": "raw"
  },
  "LlmConfig": {
    "api_url": "https://api.openai.com/v1",
    "api_key": "sk-***",
    "model": "gpt-4o-mini"
  },
  "ParsedDbInfo": {
    "db_type": "mysql",
    "host": "192.168.1.10",
    "port": 3306,
    "database": "orders",
    "username": "root",
    "password": "secret",
    "schema": null,
    "confidence": 0.93
  }
}
```

### 3.3 进度与报告结构

```json
{
  "LoadProgress": {
    "table_name": "user",
    "total_rows": 2000,
    "loaded_rows": 1000,
    "failed_rows": 0,
    "status": "loading",
    "speed": 33333.3,
    "elapsed_ms": 30
  },
  "TableReport": {
    "table_name": "employee",
    "row_count": 800,
    "success_count": 800,
    "failed_count": 0,
    "elapsed_ms": 1234,
    "speed": 648.3,
    "errors": []
  },
  "LoadReport": {
    "total_tables": 5,
    "total_rows": 12300,
    "success_rows": 12300,
    "failed_rows": 0,
    "success_rate": 1.0,
    "total_elapsed_ms": 284000,
    "avg_speed": 43310.0,
    "table_reports": ["...TableReport..."]
  },
  "PreCheckResult": {
    "check_name": "disk_space",
    "passed": true,
    "message": "源目录可用空间充足",
    "severity": "info"
  }
}
```

`LoadProgress.status` 取值：`pending`、`loading`、`completed`、`completed_with_errors`、`failed`。`PreCheckResult.severity` 取值：`info`、`warning`、`error`。

## 4. 难点解决方案

### 4.1 UTF-8 多字节字段定位

GUT 标准的 `.dat` 文件为定长字节流，字段位置以「字节」为单位。但中文字符（如「张伟」）在 UTF-8 编码下占 3 字节，若使用 `&str::chars()` 按字符切片会与 PRD 定义的字节区间错位，导致后续字段被截断或越界。

解决方案：

- 全程在 `&[u8]` 上按字节切片，提取字段后再以 `std::str::from_utf8` 转换
- 字段闭区间为 `[start - 1, end)`（`start` / `end` 均为 1-based 字节位置）
- 转换失败的行只记录警告，不中断整体解析

### 4.2 PostgreSQL 批量插入类型转换

sqlx 对 PostgreSQL 类型校验严格，将字符串绑定到 `INT` / `NUMERIC` 列时会触发「inconsistent types deduced for parameter」错误。同时数据文件中数值列右侧补空格 trim 后可能为空字符串，直接绑定会被解释为非法数值。

解决方案：

- 在生成的占位符上追加显式类型转换后缀：`Varchar` -> `::TEXT`、`Decimal` -> `::NUMERIC`、`Int` -> `::BIGINT`
- 数值列空字符串绑定为 `NULL`，避免数值转换失败
- 提供 `pg_cast_suffix` 辅助函数集中维护类型映射，调用方完全透明

### 4.3 并发加载与进度同步

加载流程是 IO 密集型异步任务，进度需要从批次循环实时推送给前端，但 Tauri `State<'_, AppState>` 的生命周期不能跨越 `tokio::spawn`，且批量插入吞吐应不受 UI 消费速度影响。

解决方案：

- 在 `start_loading` 中先 `Arc::clone` 三组共享状态，再 move 进 spawn 闭包，永远只持有 `Arc` 不持有 `State`
- 每张表创建独立的 `tokio::sync::mpsc` 通道与转发协程，加载器使用 `try_send` 非阻塞推送进度
- 转发协程通过 `Sender` drop 自然退出，外层 `await` 确保它在下一张表开始前完成，事件按表隔离不串插
- `is_loading` 上锁与赋值不跨越 `await` 边界，避免重复启动；取消标志在表与表之间检查，避免中途中断

### 4.4 数据库参数上限

PostgreSQL 单条语句最多约 65535 个参数，MySQL 也存在类似限制。批量插入若简单按固定行数拼接，会在字段数较多的表上触发参数上限。

解决方案：

- `safe_batch_size(column_count, desired_batch_size)` 以 60000 为安全阈值，动态计算实际行数：`max(1, min(desired, 60000 / column_count))`
- 在 `MysqlLoader::batch_insert` 与 `PostgresLoader::batch_insert` 中均按此行数二次分批，调用方仅指定期望批次

### 4.5 跨平台磁盘空间获取

不同操作系统的磁盘空间 API 差异较大（macOS 使用 statfs、Linux 使用 statvfs、Windows 使用 GetDiskFreeSpaceEx），引入 libc / winapi 会显著增加构建复杂度。

解决方案：

- 通过 `std::process::Command` 执行系统 `df -k` 命令并解析输出第二行第 4 列（Available），单一实现兼容 macOS 与 Linux
- 路径不存在时回退至父目录，避免命令直接失败
- Windows 环境的实现通过 feature gate 预留扩展位（当前版本仅交付 macOS / Linux 主干）

### 4.6 LLM 响应格式不稳定

不同 LLM 服务对 `response_format = json_object` 支持程度不同，部分模型仍会返回 Markdown 围栏包裹的 JSON 或带前后缀解释的文本。

解决方案：

- `parse_json_payload(raw)` 优先直接 `serde_json::from_str::<ParsedDbInfo>`
- 失败时调用 `extract_json_object` 提取首个花括号配平的 JSON 片段：跟踪 `{}` 深度、`"` 字符串范围、`\` 转义
- 双重失败返回默认 `ParsedDbInfo`，UI 层不会因模型异常而崩溃

### 4.7 数据库密码 URL 编码

数据库密码可能包含 `@` / `:` / `/` 等 URL 特殊字符，若拼接 `mysql://user:pass@host:port/db` 字符串会导致 URL 解析失败或字段错位。

解决方案：

- 使用 `MySqlConnectOptions` / `PgConnectOptions` 结构体逐项设置 `host`、`port`、`username`、`password`、`database`、`options`，从根本上避免 URL 编码问题
- PostgreSQL schema 通过 `options(["search_path", schema])` 设置，而非嵌入连接串

## 5. 配置示例

### 5.1 Tauri capabilities

`src-tauri/capabilities/default.json` 已声明以下能力：

- `core:default`、`core:window:default`：核心窗口操作
- `dialog:allow-open`：目录选择对话框
- `fs:default`、`fs:allow-read-text-file`、`fs:allow-write-text-file`：文件读写
- `shell:allow-open`：在系统中打开文件路径

新增命令需要文件系统 / shell 权限时，按需追加权限项。

### 5.2 数据库配置

```json
{
  "db_type": "postgresql",
  "host": "127.0.0.1",
  "port": 5433,
  "database": "gut_test",
  "username": "postgres",
  "password": "testpass123",
  "schema": "public"
}
```

### 5.3 LLM 配置

```json
{
  "api_url": "https://api.openai.com/v1",
  "api_key": "sk-xxxxxxxxxxxxxxxx",
  "model": "gpt-4o-mini"
}
```

兼容服务示例：

- DeepSeek：`api_url = "https://api.deepseek.com/v1"`、`model = "deepseek-chat"`
- 通义千问：`api_url = "https://dashscope.aliyuncs.com/compatible-mode/v1"`、`model = "qwen-plus"`

### 5.4 集成测试启动命令

```bash
docker run -d --name gut-test-mysql -e MYSQL_ROOT_PASSWORD=testpass123 -e MYSQL_DATABASE=gut_test -p 3307:3306 mysql:8.0
docker run -d --name gut-test-postgres -e POSTGRES_PASSWORD=testpass123 -e POSTGRES_DB=gut_test -p 5433:5432 postgres:16

cd src-tauri && cargo test --test integration_test -- --nocapture --test-threads=1
```

未启动 Docker 时，MySQL / PostgreSQL 用例会以 `eprintln!` 打印跳过原因后正常返回，不影响纯本地用例运行。

### 5.5 Makefile 构建脚本

根目录 `Makefile` 封装项目常用命令，统一开发与发布入口，所有目标声明为 `.PHONY`，默认目标为 `help`。

命令分组：

- 开发调试：`install`、`dev`、`dev-web`、`check`、`test`、`test-integration`、`lint`
- 构建打包：`build`、`build-macos`、`build-macos-arm`、`build-windows`、`build-windows-x64`、`build-linux`、`build-linux-x64`、`build-linux-arm`、`build-all`
- 数据库管理：`db-up`、`db-down`、`db-status`（基于 Docker，端口 MySQL 3307 / PostgreSQL 5433，密码 `testpass123`，库名 `gut_test`）
- 驱动打包：`bundle-drivers`（显示达梦 ODBC 驱动打包说明）
- 清理：`clean`、`clean-rust`、`rebuild`
- 帮助：`help`（默认目标）

实现要点：

- 前端代码位于 `frontend/` 子目录，所有 npm 相关命令通过 `cd frontend && ...` 形式调用
- 定义 `TAURI_BIN := ./frontend/node_modules/.bin/tauri` 变量，在项目根目录执行 Tauri CLI 以正确定位 `src-tauri/`
- 通过 `cd src-tauri && cargo ...` 形式调用 Rust 工具链，避免依赖根目录 Cargo workspace
- `tauri.conf.json` 中 `beforeDevCommand` / `beforeBuildCommand` 使用 `cd ../frontend && npm run ...` 形式（相对于 `src-tauri/` 目录执行）
- Docker 容器启动命令带 `|| true`，已存在时不中断脚本；停止/移除统一重定向 `2>/dev/null` 屏蔽不存在的容器报错
- `test-integration` 在执行前后自动启停容器，构成自包含的端到端验证闭环
- `help` 目标使用 `@echo` 打印中文分类清单，避免 `make` 默认回显规则本身
- `build-windows-x64` 在 macOS 上切换为 GNU 交叉编译路径：先检查 `mingw-w64` 与 `makensis`，再以 `x86_64-pc-windows-gnu` 目标和 `x86_64-w64-mingw32-gcc` 链接器调用 Tauri，生成 Windows 可执行文件与 NSIS 安装包；非 macOS 平台继续保留 `x86_64-pc-windows-msvc`
- `build-linux-x64` 与 `build-linux-arm` 在 macOS 上通过 Docker 运行 Linux 构建环境，分别以 `linux/amd64` 与 `linux/arm64` 平台执行 Tauri 打包；非 macOS 平台仍直接调用本机 Tauri 目标
- `build-all` 直接串联各平台目标，形成统一入口
- `clean` 同时删除 `frontend/node_modules`、`frontend/dist`、`src-tauri/target`，并执行 `cargo clean`，确保彻底回到初始状态

## 6. 项目结构

```
gut-loader/
├── docs/
│   ├── PRD.md                          # 需求文档
│   ├── IMPLEMENTATION.md               # 实现文档
│   └── changelog.md                    # 变更记录
├── example_data/                       # 5 组样例数据（dat.gz + flg）
├── frontend/                           # 前端工程目录
│   ├── src/                            # 前端源码
│   │   ├── components/
│   │   │   ├── ui/                     # shadcn/ui 基础组件
│   │   │   ├── DatabaseConfig.tsx
│   │   │   ├── FileSelector.tsx
│   │   │   ├── LLMConfig.tsx
│   │   │   ├── LoadingProgress.tsx
│   │   │   ├── PreCheckPanel.tsx
│   │   │   └── ReportView.tsx
│   │   ├── hooks/
│   │   │   ├── useLoadingEvents.ts
│   │   │   └── useTauriCommands.ts
│   │   ├── lib/
│   │   │   ├── types.ts
│   │   │   └── utils.ts
│   │   ├── pages/
│   │   │   └── HomePage.tsx
│   │   ├── stores/
│   │   │   └── appStore.ts
│   │   ├── App.tsx
│   │   ├── main.tsx
│   │   └── index.css
│   ├── index.html
│   ├── package.json
│   ├── vite.config.ts
│   ├── tsconfig.json
│   ├── tsconfig.node.json
│   ├── tailwind.config.js
│   ├── postcss.config.js
│   └── components.json                 # shadcn/ui 配置
├── src-tauri/                          # 后端源码
│   ├── capabilities/default.json
│   ├── icons/                          # 应用图标
│   ├── bundled-drivers/                # 随安装包打包的驱动资源
│   │   ├── odbcinst.ini                # ODBC 驱动配置模板（DRIVER_PATH 占位符）
│   │   └── dm-odbc/{linux,windows}/{arm64,x64}/  # 达梦 ODBC 驱动二进制存放目录
│   ├── src/
│   │   ├── parser/{mod,flg,dat}.rs
│   │   ├── database/{mod,mysql,postgres,oracle,dm}.rs
│   │   ├── validator/{mod,disk,file}.rs
│   │   ├── llm/{mod,parser}.rs
│   │   ├── loader/{mod,batch}.rs
│   │   ├── report/mod.rs
│   │   ├── commands/mod.rs
│   │   ├── models.rs
│   │   ├── lib.rs
│   │   └── main.rs
│   ├── tests/integration_test.rs       # 端到端集成测试
│   ├── build.rs
│   ├── Cargo.toml
│   └── tauri.conf.json
├── Makefile                            # 构建脚本（开发/构建/测试/清理统一入口）
├── README.md
└── .gitignore
```

## 7. 与需求文档对应关系

| PRD 章节 | 实现章节 |
| --- | --- |
| 2.1 文件解析（GUT 标准格式） | 2.1 文件解析模块；3.1 解析层结构 |
| 2.2 目录扫描与自动识别 | 2.1 文件解析模块（`scan_directory`） |
| 2.3 前置检查 | 2.2 前置检查模块；4.5 跨平台磁盘空间获取 |
| 2.4 多数据库支持 | 2.3 数据库模块；4.2 PG 类型转换；4.4 参数上限；4.7 密码编码 |
| 2.5 LLM 智能识别 | 2.4 LLM 集成模块；4.6 响应格式不稳定 |
| 2.6 数据加载 | 2.5 数据加载模块；4.3 并发加载与进度同步 |
| 2.7 报告生成 | 2.6 报告生成模块 |
| 2.8 桌面 GUI 应用 | 2.8 前端 UI；2.7 Tauri 命令层 |
