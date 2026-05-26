# GUT 数据加载工具 - 需求文档（PRD）

## 1. 项目概述

### 1.1 简介

GUT 数据加载工具（gut-loader）是一款基于 GUT 数据交换标准的通用桌面端数据加载入库工具。围绕统一卸数格式（`.flg` 元数据 + `.dat.gz` 定长压缩数据）的双文件协议，提供端到端的目录扫描、文件解析、前置校验、批量入库、报告生成能力，统一覆盖 macOS、Windows、Linux 三端桌面环境。

工具以向导式的桌面 GUI 形式交付，操作者可在不接触命令行与脚本的情况下，完成从源目录选择到目标库写入、再到查看与导出报告的全流程。

### 1.2 核心特性

- 标准化的 GUT 双文件解析：自动识别同前缀的 `.flg` 与 `.dat.gz` 文件对，按字节偏移读取定长字段，原生兼容 UTF-8 多字节内容（如汉字）
- 多数据库统一适配：在统一抽象层下支持 MySQL、PostgreSQL、openGauss、TXSQL、TDSQL、GaussDB、Oracle；达梦 DM 仅在 Windows x64、Linux x64 与 Linux arm64 版本中提供
- 高性能批量加载：基于 Tokio 异步运行时与连接池，按表分批写入，自动根据字段数量动态计算批次行数以规避数据库参数上限
- 完整的前置校验：覆盖磁盘空间、文件命名、FLG 可解析性、字段位置连续性、GZIP 完整性、行数一致性等多维度
- LLM 智能识别：兼容 OpenAI Chat Completions 协议，从一段中文或英文自然语言中识别并填充数据库连接信息
- 实时进度与报告：单表与总进度实时同步、终端风格日志面板、报告页提供柱状图与饼图可视化、支持 JSON 与文本摘要导出
- 现代化桌面 UI：Tauri v2 + React 18 + shadcn/ui，5 步线性向导，统一的 amber 信号色主题
- 协作式可取消：加载过程中可在表与表之间安全终止，已写入数据保持完整

### 1.3 解决的问题

- 各业务系统重复实现入库逻辑，编码缺陷与性能差异频发
- 入库前缺乏统一前置校验，问题在写入中段暴露，回滚成本高
- 数据库连接信息分散在各自文档与 IM 中，手工录入易错
- 缺乏跨数据库一致的写入抽象，切换库种需重新开发

## 2. 功能说明

### 2.1 文件解析（GUT 标准格式）

#### 2.1.1 FLG 元数据解析

解析符合 `<TABLE>.<YYYYMMDD>.<HHMMSS>.<SEQUENCE>.flg` 命名约定的元数据文件，输出表名、文件名、文件大小、行数、创建时间、SELECT 语句、行字节长度、字段个数与字段定义列表。

字段定义遵循 `序号$$字段名$$数据类型$$（起始位置,结束位置）` 行格式，支持的数据类型为：

- `VARCHAR(n)`：变长字符（实际按字节定长存储）
- `DECIMAL(m,n)`：定点数
- `INT(n)`：整数

实现兼容半角 `(s,e)` 与全角 `（s,e）` 括号、半角逗号与全角逗号；非法或格式错误的字段定义会被记录为警告日志，不阻断整体解析；当字段定义数量与 `COLUMNCOUNT` 不一致时输出 warning。

#### 2.1.2 DAT 定长数据解析

解析 `.dat` 或 `.dat.gz` 文件（按扩展名自动判断是否走 gzip 解码），按 `\r\n` 切分行（行终止符不计入 ROWLENGTH），并以字节闭区间 `[start-1, end)` 提取每个字段。VARCHAR 字段右侧空格自动去除。

提供两种调用方式：

- 一次性解析：返回全部 `DataRow`，适合中小文件
- 流式分批：按 `batch_size` 通过回调输出，适合大文件分批入库以控制内存占用

非法行（长度不符、字段越界、非合法 UTF-8）会被记录为警告日志后跳过，不中断整体解析。

#### 2.1.3 使用示例

```text
employee.20260421.000000.0000.flg
employee.20260421.000000.0000.dat.gz
```

解析得到 `FlgMetadata { table_name: "employee", row_length: 212, column_count: 6, columns: [...] }`，再驱动 DAT 解析输出 800 行 `DataRow`。

### 2.2 目录扫描与自动识别

输入：包含若干 `.flg` 与 `.dat.gz` 的目录。

行为：按命名规则 `<TABLE>.<YYYYMMDD>.<HHMMSS>.<SEQUENCE>.<EXT>` 配对同前缀的 `.flg` 与 `.dat.gz`；仅当配对成功时输出 `GutFilePair`，孤立文件记入日志；输出列表按 `(表名, 日期, 时间, 序号)` 升序排序，便于稳定显示。

使用示例：

- 选择目录 `./example_data` 后，扫描得到 5 组文件对：employee（800 行）、order（3000 行）、product（1500 行）、transaction（5000 行）、user（2000 行），合计 12300 行

### 2.3 前置检查

加载启动前，工具会按顺序执行多维度的前置检查，所有检查结果聚合为 `Vec<PreCheckResult>` 返回前端。前端按 `severity` 字段决定阻塞策略：`error` 级别阻塞下一步、`warning` 级别提示但允许继续、`info` 级别仅展示。

#### 2.3.1 磁盘空间检查

| 检查 | 阈值 | 严重级别 |
| --- | --- | --- |
| 源目录磁盘可用空间 | 不低于 `.dat.gz` 文件总大小的 2 倍（解压缓冲） | warning |
| 目标磁盘可用空间 | 不低于 1 GB | warning |

通过系统 `df -k` 命令获取可用空间，兼容 macOS 与 Linux。当目标路径不存在时，回退至父目录执行检查。

#### 2.3.2 文件格式与一致性检查

| 检查项 | 检查内容 | 严重级别 |
| --- | --- | --- |
| 目录存在性 | 目录是否存在且可读 | error |
| 文件扫描 | 是否发现至少一组文件对 | error |
| 文件命名格式 | 符合 `table.YYYYMMDD.HHMMSS.SEQ.ext` | warning |
| FLG 可解析 | FLG 文件能正确解析 | error |
| DAT 文件存在 | `.dat.gz` 文件存在 | error |
| 字段数量 | `columns.len() == column_count` | error |
| 字段位置范围 | 起始位置从 1 开始、相邻无间隙、末位置 = `ROWLENGTH` | error |
| GZIP 完整性 | 文件可正常解压（取前 5 行验证） | error |
| 行数匹配 | 实际行数 == `ROWCOUNT` | warning |

### 2.4 多数据库支持

通过统一的 `DatabaseLoader` 抽象屏蔽各数据库差异，覆盖连接测试、自动建表、批量插入、行数查询、连接关闭五大能力。

| 数据库 | 协议 | 默认端口 | 备注 |
| --- | --- | --- | --- |
| MySQL | 原生 MySQL | 3306 | sqlx 原生支持 |
| TXSQL | MySQL 兼容 | 3306 | 复用 MySQL 适配器 |
| TDSQL | MySQL 兼容 | 3306 | 复用 MySQL 适配器 |
| PostgreSQL | 原生 PG | 5432 | sqlx 原生支持，可指定 schema |
| openGauss | PG 兼容 | 5432 | 复用 PG 适配器 |
| GaussDB | PG 兼容 | 5432 | 复用 PG 适配器 |
| Oracle | TNS | 1521 | oracle-rs 0.1 纯 Rust TNS 驱动，零系统依赖（无需 Instant Client / OCI / ODPI-C） |
| 达梦 DM | ODBC | 5236 | 仅 Windows x64、Linux x64 与 Linux arm64 支持；macOS 版本不展示达梦入口 |

DDL 类型映射：

| 内部类型 | MySQL DDL | PostgreSQL DDL | Oracle DDL | 达梦 DM DDL |
| --- | --- | --- | --- | --- |
| `Varchar(n)` | `VARCHAR(n)` | `VARCHAR(n)` | `VARCHAR2(n)` | `VARCHAR(n)` |
| `Decimal(m,n)` | `DECIMAL(m,n)` | `NUMERIC(m,n)` | `NUMBER(m,n)` | `DECIMAL(m,n)` |
| `Int(n)` | `BIGINT` | `BIGINT` | `NUMBER(19)` | `BIGINT` |

#### 2.4.1 自动建表

`CREATE TABLE IF NOT EXISTS` 语义，存在同名表时跳过；MySQL 表名/列名采用反引号转义，PostgreSQL 采用双引号转义并支持 `"schema"."table"` 限定格式；Oracle 与达梦采用双引号转义，建表前查询 `user_tables` / `all_tables` 数据字典判断表是否已存在以模拟 `IF NOT EXISTS` 语义。

#### 2.4.2 批量插入

- 占位符格式：MySQL 使用 `?`、PostgreSQL 使用编号 `$1, $2, ...` 并附加 `::TEXT` / `::NUMERIC` / `::BIGINT` 显式类型转换后缀、Oracle 使用位置参数 `:1, :2, ...` 并对数值列包装 `TO_NUMBER(NULLIF(:n, ''))`、达梦使用 prepared statement 逐行绑定参数
- PostgreSQL 数值列空字符串自动转换为 `NULL`，避免数值类型转换错误；Oracle 通过 `NULLIF(:n, '')` 实现同等效果
- 批次大小默认 1000 行，根据 `字段数 × 行数 ≤ 60000` 自适应缩减，确保不触达数据库参数上限
- 单批失败不中断后续批次，错误聚合至 `TableReport.errors`

### 2.5 LLM 智能识别

#### 2.5.1 协议兼容性

基于 OpenAI Chat Completions 协议，仅需配置 `api_url`（base URL）、`api_key`、`model` 三项参数即可适配 OpenAI、DeepSeek、通义千问等任意 OpenAI 兼容服务。

- 实际请求地址为 `{api_url}/chat/completions`，自动修饰尾部斜杠
- 默认 30 秒超时
- 启用 `response_format = json_object` 与 `temperature = 0.0`，保证输出结构稳定与可复现性
- 401 / 403 状态码识别为认证失败，其他非 2xx 返回明确的接口异常信息

#### 2.5.2 数据库连接信息提取

输入：一段自然语言文本，例如「请连接 192.168.1.10 上的 MySQL，端口 3306，账号 root，密码 secret，库名 orders」。

输出：`ParsedDbInfo`，包含 `db_type`、`host`、`port`、`database`、`username`、`password`、`schema`、`confidence`。

- `db_type` 限定为 `mysql / postgresql / opengauss / txsql / tdsql / gaussdb / oracle / dameng` 之一
- 无法识别的字段返回 `null`
- 响应解析降级：直接 JSON 反序列化失败时，从响应中提取首个花括号配平的 JSON 片段再解析；仍失败则返回默认空结果，避免 UI 层异常
- 输入为空时不发起 LLM 请求，直接返回默认值

#### 2.5.3 配置连通性验证

调用 `validate_config()` 发送内容为 `Hello` 的探活请求验证可用性：`api_url` / `api_key` / `model` 为空返回 `Err`；网络失败返回 `Ok(false)`；成功返回 `Ok(true)`。

#### 2.5.4 转换为运行时配置

`ParsedDbInfo::to_database_config()` 将识别结果转换为 `DatabaseConfig`，缺失字段使用默认值（`mysql` / `localhost` / `3306` / 空串）。

### 2.6 数据加载

#### 2.6.1 加载编排

`load_table` 函数为单表完整加载入口，以 dat.gz 实际文件大小为判据依据自动选择加载策略：

- 文件小于等于 100MB：调用 `load_table_inmemory`，一次性解析全部行后分批插入，减少小文件场景下重复 IO 与系统调用开销
- 文件大于 100MB：调用 `load_table_streaming`，边读边写，内存占用始终控制在单批数据量级（默认 1000 行）

两条路径均依次执行：

1. 解析 FLG 元数据
2. 自动建表（已存在则跳过）
3. 查询表内当前行数（断点续传基础）
4. 按路径解析 DAT，跳过已加载的行数
5. 分批插入，每批结束后通过 `mpsc::Sender<LoadProgress>` 推送进度
6. 聚合返回 `TableReport`

#### 2.6.2 大文件流式处理

对于超过 100MB 阈值的 dat.gz 文件，工具采用流式读取与分片入库策略：

- 仅以 `GzDecoder + BufReader` 逐行解压与读取，不会将全量行一次性加载进内存
- 每凑齐一批立即调用 `batch_insert` 入库并清空本地缓冲，处理完末尾不满一批的数据后返回
- 阈值可通过公开常量 `STREAMING_THRESHOLD_BYTES` 读取，便于下游测试与环境变量定制
- 运行时会记录 `info!` 日志说明本次加载走了哪条路径，便于运维诊断

#### 2.6.3 断点续传

假设数据按追加模式写入（不删除、不更新），通过 `SELECT COUNT(*)` 取得已有行数，跳过解析结果中的对应前缀，避免重复入库。

#### 2.6.4 协作式取消

加载启动后，前端可通过取消命令设置取消标志；后台任务在每张表完成后检查并退出主循环。当前表内不会被中断，避免部分批次状态不一致。

#### 2.6.5 实时进度

| 字段 | 说明 |
| --- | --- |
| `table_name` | 当前正在加载的表 |
| `total_rows` | 当前表的总行数 |
| `loaded_rows` | 当前表的已成功行数 |
| `failed_rows` | 当前表的失败行数 |
| `status` | `pending / loading / completed / completed_with_errors / failed` |
| `speed` | 实时速度（行/秒） |
| `elapsed_ms` | 当前表已耗时（毫秒） |

### 2.7 报告生成

#### 2.7.1 报告聚合

`ReportGenerator` 在创建时记录起始时间戳；通过 `add_table_report` 累积单表执行结果；`generate()` 输出汇总报告 `LoadReport`：总表数、总行数、成功行数、失败行数、成功率、总耗时、平均速率，并附带每张表的明细 `TableReport`。

成功率与平均速率在分母为 0 时统一返回 0.0，避免 `NaN` / `Inf` 污染输出。

#### 2.7.2 持久化与回读

- `export_json(path)` 将 `LoadReport` 序列化为 pretty JSON 写入磁盘，目标父目录不存在时自动创建
- `load_report_from_file(path)` 从指定 JSON 文件回读 `LoadReport`，便于历史报告查看

#### 2.7.3 文本摘要

`export_summary()` 生成可直接打印的中文报告：标题块（含生成时间）、汇总统计、各表详情表、错误详情。错误详情按表分组展示，单表最多输出前 10 条错误，超出部分以「... 及其他 N 条错误」截断提示。耗时显示格式化为 `Xms` / `X.Ys` / `XmY.Zs`。

### 2.8 桌面 GUI 应用

#### 2.8.1 5 步线性向导

| 步骤 | 标题 | 核心交互 |
| --- | --- | --- |
| 01 | 选择源文件目录 | 调用 Tauri Dialog 选择目录、扫描 GUT 文件对、表格化展示结果 |
| 02 | 配置目标数据库 | 多类型数据库选择、参数表单、测试连接、LLM 智能填充 |
| 03 | 前置检查 | 进入步骤时自动触发预检、按 severity 渲染检查项、阻塞错误时禁止下一步、支持手动重新运行 |
| 04 | 执行加载 | 总进度 + 表级进度、实时日志面板、概览统计四联卡 |
| 05 | 查看报告 | 汇总卡片、每表详情表格、速率柱状图、成功失败饼图、JSON 导出 |

顶部步骤条支持点击回跳；每步状态分为 `todo / active / done`：已完成步骤以 emerald 色调显示对勾，活跃步骤以前景反色填充。步骤完成态基于各步骤的实际验证条件判断，而非仅以步骤序号小于当前步骤为依据：

| 步骤 | 完成条件 |
| --- | --- |
| 01 源文件 | 扫描得到至少一组文件对 |
| 02 目标库 | host、port、database、username 四项均有值 |
| 03 前置检查 | 检查已执行且无 severity=error 的未通过项 |
| 04 执行加载 | 报告已生成（report 不为空） |
| 05 查看报告 | 始终不显示完成态（为流程终点） |

#### 2.8.2 数据库类型与默认端口

数据库类型切换时自动填充默认端口：

| 类型 | 默认端口 |
| --- | --- |
| MySQL / TXSQL / TDSQL | 3306 |
| PostgreSQL / openGauss / GaussDB | 5432 |
| Oracle | 1521 |
| 达梦 DM | 5236 |

#### 2.8.3 数据库配置保存与复用

数据库配置页支持将当前连接参数保存为命名配置，以便日后快速加载复用：

- 连接参数卡片标题行提供「加载已保存配置」下拉列表，每个选项显示配置名称与连接信息摘要（数据库类型 @ 主机:端口），选择后自动填充表单字段
- 提供「保存」按钮，点击后弹出对话框输入配置名称，确认后将当前配置持久化到本地 localStorage
- 每条已保存配置旁提供删除按钮，可移除不再需要的配置
- 安全策略：保存时密码字段自动置空，加载时密码同样为空，需用户每次重新输入
- 所有保存/加载/删除操作通过 toast 提示操作结果

#### 2.8.4 LLM 智能识别面板

数据库配置页内嵌 LLM 智能识别面板，默认展开：

- 面板标题栏右侧提供「配置 LLM」按钮与独立的折叠/展开按钮；点击「配置 LLM」后弹出「LLM 服务配置」对话框，于弹窗内配置 API URL、API Key（带显示/隐藏切换）、模型名称，各字段初始为空，placeholder 显示示例值（如 `https://api.openai.com/v1`、`gpt-4o-mini`）
- LLM 配置自动持久化保存至本地 localStorage，下次启动时自动加载已保存的配置，配置变更时在弹窗内显示“已自动保存”反馈
- 当 API URL 或模型未配置时，「配置 LLM」按钮右上角显示 amber 色小圆点作为状态指示
- 弹窗内提供独立的 LLM 连接测试按钮
- 面板展开区仅保留连接信息文本 textarea 与「智能识别」按钮；在 textarea 中粘贴自然语言连接信息后点击「智能识别」，由后端 `parse_db_info` 命令解析并自动覆盖表单字段，识别成功后以 toast 提示具体填充字段名
- 当 API URL 或模型未配置时，智能识别按钮禁用并显示提示文字

#### 2.8.5 设计系统与可视化

- 字体：标题与正文使用 Inter Tight，所有数值与代码采用 JetBrains Mono 等宽显示
- 主题：基于 shadcn 的双主题 CSS 变量，新增 amber 信号色作为 `--accent`，emerald 用于成功态、destructive 用于失败态
- 全局背景：极淡网格纹理 + 顶部琥珀色光晕
- 报告页：使用 recharts 渲染速率柱状图与成功/失败饼图，颜色与全局主题对齐
- 操作反馈：所有 IPC 调用均带 loading / success / error 三态，通过 sonner toast 反馈

#### 2.8.6 跨平台视觉一致性

为保证在 macOS、Windows、Linux 三端的视觉一致性，全局样式层包含以下跨平台兼容策略：

| 策略 | 说明 |
| --- | --- |
| 字体渲染 | 启用 `-webkit-font-smoothing: antialiased`、`-moz-osx-font-smoothing: grayscale`、`text-rendering: optimizeLegibility`，统一各平台字体边缘表现 |
| 滚动条 | 同时定义 Webkit 滚动条样式（`::-webkit-scrollbar`）与 Firefox `scrollbar-width: thin` + `scrollbar-color`，保证主流浏览器引擎统一 |
| 文本选中 | `::selection` 使用 accent 色 30% 透明度作为背景色，避免各平台默认选中色差异 |
| 高对比度模式 | 通过 `@media (forced-colors: active)` 为 `focus-visible` 元素提供 2px 实线轮廓，兼容 Windows 高对比度模式 |
| 点击反馈 | `-webkit-tap-highlight-color: transparent` 移除移动端默认反射高亮 |

#### 2.8.7 实时事件订阅

前端通过 Tauri 事件系统订阅四类后端事件，并同步至 zustand store：

| 事件 | 载荷 | 用途 |
| --- | --- | --- |
| `loading-progress` | `LoadProgress` | 单表实时进度 |
| `table-completed` | `TableReport` | 单表加载完成 |
| `loading-completed` | `LoadReport` | 全部表完成、写入最终报告 |
| `loading-error` | `String` | 致命错误或单表错误 |

### 2.9 构建与开发命令

项目根目录提供 `Makefile` 作为统一构建入口，封装 npm 与 cargo 的常用操作，支持开发调试、多平台构建、测试数据库管理与一键清理。前端代码位于 `frontend/` 子目录，Tauri CLI 通过 `./frontend/node_modules/.bin/tauri` 在项目根目录执行以正确定位 `src-tauri/`：

| 类别 | 命令 | 说明 |
| --- | --- | --- |
| 开发 | `make install` | 同时安装前端 npm 依赖（`cd frontend && npm install`）与 Rust 依赖（cargo fetch） |
| 开发 | `make dev` | 启动 Tauri 桌面端开发模式（前端 + 后端热重载） |
| 开发 | `make dev-web` | 仅启动 Vite 前端开发服务器 |
| 开发 | `make check` | 运行 `cargo check` 校验 Rust 编译 |
| 开发 | `make test` | 运行后端全部 cargo 测试（单元 + 集成） |
| 开发 | `make test-integration` | 拉起 Docker MySQL/PostgreSQL 后运行集成测试，结束后自动停止与移除容器 |
| 开发 | `make lint` | `cargo clippy -D warnings` 与 `tsc --noEmit` 双侧静态检查 |
| 构建 | `make build` | 构建当前平台 Tauri 应用 |
| 构建 | `make build-macos-arm` | 构建 macOS Apple Silicon 包 |
| 构建 | `make build-windows-x64` / `build-linux-x64` / `build-linux-arm` | 在 macOS 上分别通过 mingw-w64 或 Docker 构建 Windows x64 / Linux GNU x64 / Linux GNU arm64 包；在其他平台仍可按原生环境执行 |
| 构建 | `make build-all` | 按声明矩阵执行全部构建目标 |
| 数据库 | `make db-up` / `db-down` / `db-status` | 管理本地 Docker 测试数据库（MySQL 3307、PostgreSQL 5433） |
| 驱动打包 | `make bundle-drivers` | 显示达梦 ODBC 驱动打包说明 |
| 清理 | `make clean` / `clean-rust` / `rebuild` | 清理 frontend/node_modules、frontend/dist、cargo target；或仅清理 Rust 缓存；或清理后重新安装 |
| 帮助 | `make help` | 默认目标，输出全部命令的中文说明 |

说明：所有 Tauri CLI 调用通过 `./frontend/node_modules/.bin/tauri` 在项目根目录执行，确保正确定位 `src-tauri/` 目录；Windows x64 在 macOS 上使用 `x86_64-pc-windows-gnu` + `mingw-w64` 交叉编译并生成 NSIS 安装包；Linux x64 / Linux arm64 在 macOS 上通过 Docker 容器构建；跨平台打包前需准备对应 Rust target 与系统级打包依赖。

## 3. 技术要求

| 项目 | 要求 |
| --- | --- |
| Rust 工具链 | edition 2021，stable 通道 |
| Node.js | 18+ |
| 包管理器 | npm（lockfile 已纳入 git） |
| Tauri | v2，需配置 capabilities 权限模型 |
| 前端 | TypeScript 5.7、React 18、Vite 6 |
| 数据库驱动 | sqlx 0.8（mysql + postgres）；Oracle 基于 oracle-rs 0.1（纯 Rust TNS 协议，默认参与编译，无需 Oracle Instant Client / OCI / ODPI-C）；达梦基于 odbc-api 8 默认参与编译，驱动二进制随 Tauri 资源打包分发 |
| 平台支持 | macOS、Windows、Linux |
| 测试环境 | Docker（可选，用于 MySQL / PostgreSQL 集成测试） |
| 构建入口 | 根目录 `Makefile`，前端代码位于 `frontend/` 子目录，覆盖开发调试、多平台构建、测试数据库管理与清理 |

## 4. 限制与约束

- 达梦 DM 适配器仅在 Windows x64、Linux x64 与 Linux arm64 版本中启用；macOS 版本不编译达梦 ODBC 适配器，也不展示达梦数据库类型。Oracle 适配器基于 oracle-rs 0.1（纯 Rust TNS 协议）默认参与编译，零系统依赖运行
- 加载假设为追加模式（不删除、不更新），断点续传基于 `SELECT COUNT(*)` 行数对比，不适用于乱序更新场景
- 单表 `errors` 列表无截断，理论上对应批次行数；导出 JSON 时同样保留全部错误，需关注磁盘开销
- LLM 智能识别依赖外部模型服务，离线环境不可用；无法识别时会返回带默认值的 `DatabaseConfig`，需要操作者手工核对
- DAT 文件需符合行终止符 `\r\n` 与字节闭区间字段的 GUT 规范，行长度不一致的行将被跳过而非纠错

## 5. 使用注意事项

- 项目根目录下的 `example_data/` 提供 5 组合计 12300 行的样例数据（dat.gz + flg），用于功能演示与回归测试
- 入库前应保证目标磁盘剩余空间不低于解压后数据量的 1.2 倍
- 数据库密码可能包含 URL 特殊字符（`@` / `:` / `/` 等），工具内部使用 `MySqlConnectOptions` / `PgConnectOptions` 构造连接而非拼接 URL，无须用户手动 URL 编码
- LLM 解析为辅助手段，识别结果应人工核对后再点击「测试连接」与「下一步」
- 加载过程中点击「停止加载」会在当前表完成后终止，已写入的数据不会回滚
- 报告 JSON 文件名建议带时间戳（如 `gut-load-report-2026-05-26.json`），便于历史归档
