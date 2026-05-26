# GUT 数据加载工具（gut-loader）

> 基于行内标准 GUT 数据交换格式的通用桌面端数据加载入库工具，覆盖 macOS / Windows / Linux 三端。

[![Tauri](https://img.shields.io/badge/Tauri-v2-FFC131)](https://tauri.app/)
[![React](https://img.shields.io/badge/React-18-61DAFB)](https://react.dev/)
[![TypeScript](https://img.shields.io/badge/TypeScript-5.7-3178C6)](https://www.typescriptlang.org/)
[![Rust](https://img.shields.io/badge/Rust-edition%202021-000000)](https://www.rust-lang.org/)

## 📦 项目介绍

GUT 数据加载工具围绕统一卸数标准（`.dat.gz` 数据 + `.flg` 元数据）提供端到端的解析、校验、入库与报告能力。它以向导式的桌面 GUI 形式交付，操作者可在不接触命令行与脚本的前提下，完成从源目录选择到目标库写入、再到查看与导出报告的全流程，旨在解决各业务系统自行实现导入逻辑导致的编码缺陷与效率不一问题。

更详细的功能与实现细节请参阅：

- [docs/PRD.md](docs/PRD.md) — 需求文档
- [docs/IMPLEMENTATION.md](docs/IMPLEMENTATION.md) — 实现文档
- [docs/changelog.md](docs/changelog.md) — 变更记录

## ✨ 项目亮点

- 🚀 **高性能并发加载**：Rust + Tokio 异步运行时 + sqlx 连接池，PostgreSQL 实测 5 表 12300 行平均速率约 43000 行/秒
- 🧰 **多数据库统一适配**：MySQL / PostgreSQL / openGauss / TXSQL / TDSQL / GaussDB / Oracle / 达梦 DM 共 8 种
- 🛡️ **完整前置校验**：磁盘空间、文件命名、FLG 可解析性、字段位置连续性、GZIP 完整性、行数一致性
- 🤖 **LLM 智能识别**：兼容 OpenAI Chat Completions 协议，从一段自然语言中识别并填充数据库连接信息
- 🌏 **UTF-8 原生支持**：按字节偏移读取定长字段，原生兼容中文等多字节字符
- 📊 **可视化报告**：四联汇总卡 + 速率柱状图 + 成功/失败饼图，一键导出 JSON
- 🎨 **现代桌面 UI**：Tauri v2 + React 18 + TypeScript + shadcn/ui，5 步线性向导
- 🔁 **断点续传与协作式取消**：基于行数的断点续传，加载中可在表与表之间安全终止

## 🧱 技术栈

| 层 | 技术 |
| --- | --- |
| 桌面框架 | Tauri v2 |
| 前端 | React 18、TypeScript 5.7、Vite 6、Tailwind CSS、shadcn/ui、Zustand、recharts、sonner |
| 后端 | Rust（edition 2021）、tokio、sqlx 0.8（mysql + postgres）、reqwest、flate2、tracing、anyhow / thiserror |
| 商业数据库 | Oracle（oracle-rs 纯 Rust TNS 驱动）、达梦 DM（ODBC 驱动内置打包） |

## 🚀 快速开始

### 环境要求

- Node.js 18 及以上
- Rust stable（推荐通过 [rustup](https://rustup.rs/) 安装）
- 平台依赖请参考 [Tauri v2 prerequisites](https://v2.tauri.app/start/prerequisites/)
- （可选）Docker，用于运行 MySQL / PostgreSQL 集成测试

### 安装与启动

项目根目录提供 [`Makefile`](Makefile) 作为统一构建入口，封装 npm 与 cargo 的常用操作，推荐通过 `make` 命令完成全部开发工作。

```bash
# 1. 安装所有依赖（前端 npm + Rust cargo fetch）
make install

# 2. 启动 Tauri 桌面端开发模式（前端 + 后端热重载）
make dev

# 3. 构建发布版本（当前平台）
make build

# 4. 查看全部可用命令
make help
```

构建产物位于 `src-tauri/target/release/bundle/`，按平台输出 `.app` / `.dmg`（macOS）、`.msi` / `.exe`（Windows）、`.deb` / `.AppImage`（Linux）。

## 🧰 构建与开发命令

本项目使用 Makefile 统一管理所有构建、测试、清理与数据库操作命令。命令分为五大类：开发调试、构建打包、测试数据库管理、清理、帮助。

### 🧪 开发调试

| 命令 | 说明 |
|------|------|
| `make install` | 安装所有依赖：`cd frontend && npm install` + `cargo fetch` |
| `make dev` | 启动 Tauri 桌面端开发模式（前端 + 后端热重载） |
| `make dev-web` | 仅启动 Vite 前端开发服务器（不启动 Tauri 窗口） |
| `make check` | 运行 `cargo check`，快速验证 Rust 编译 |
| `make test` | 运行 `cargo test`，执行后端全部单元与集成测试 |
| `make test-integration` | 拉起 Docker MySQL/PostgreSQL 容器后运行集成测试，结束后自动停止与移除容器 |
| `make lint` | 双侧静态检查：`cargo clippy -D warnings` + `tsc --noEmit` |

### 📦 构建打包

| 命令 | 说明 |
|------|------|
| `make build` | 构建当前平台 Tauri 应用 |
| `make build-macos` | 构建 macOS 通用二进制（arm64 + x86_64） |
| `make build-macos-arm64` | 构建 macOS Apple Silicon（aarch64-apple-darwin） |
| `make build-macos-x64` | 构建 macOS Intel（x86_64-apple-darwin） |
| `make build-windows` | 构建 Windows MSVC 包（x86_64-pc-windows-msvc，需对应平台或交叉编译环境） |
| `make build-linux` | 构建 Linux GNU 包（x86_64-unknown-linux-gnu，需对应平台或 Docker） |
| `make build-all` | 在当前机器构建所有可用平台（默认 macOS arm64 + x64） |
| `make bundle-drivers` | 显示达梦 ODBC 驱动打包说明 |

> 跨平台目标依赖对应 Rust 三元组已通过 `rustup target add <triple>` 安装；推荐在对应 OS 或 CI 环境执行。

### 🐳 测试数据库管理

基于 Docker 的本地测试数据库，使用与默认 3306/5432 不冲突的端口（MySQL 3307、PostgreSQL 5433），密码统一为 `testpass123`，库名为 `gut_test`。

| 命令 | 说明 |
|------|------|
| `make db-up` | 启动 MySQL 8.0（端口 3307）与 PostgreSQL 16（端口 5433）测试容器 |
| `make db-down` | 停止并移除上述测试容器 |
| `make db-status` | 查看测试容器运行状态 |

### 🧹 清理

| 命令 | 说明 |
|------|------|
| `make clean` | 清理 `frontend/node_modules`、`frontend/dist`、`src-tauri/target` |
| `make clean-rust` | 仅清理 Rust 构建缓存（`cargo clean`） |
| `make rebuild` | 等价于 `make clean && make install`，全量重新初始化 |

### ❔ 帮助

| 命令 | 说明 |
|------|------|
| `make help` | 默认目标，输出全部命令的中文分类清单 |
| `make`（无参数） | 等同于 `make help` |

### 🚧 常用工作流示例

**首次拉取代码 / 新成员上手：**

```bash
make install        # 一键安装前后端依赖
make check          # 快速验证 Rust 后端可编译
make dev            # 启动桌面开发模式
```

**日常开发循环：**

```bash
make dev            # 修改代码 -> 自动热重载
make lint           # 提交前的静态检查
make test           # 运行单元测试
```

**带数据库的集成测试：**

```bash
# 方式 A：一键全流程（推荐 CI 环境）
make test-integration

# 方式 B：保留容器，手动控制（推荐本地反复调试）
make db-up
cd src-tauri && cargo test --test integration_test -- --nocapture
make db-down
```

**多平台构建发布：**

```bash
make build-macos          # macOS 通用二进制
make build-macos-arm64    # 仅 Apple Silicon
make build-windows        # Windows（需对应环境）
make build-linux          # Linux（需对应环境）
make build-all            # 当前机器所有可构建平台
```

**遇到奇怪的构建问题，重置环境：**

```bash
make rebuild        # 清理所有缓存并重新安装依赖
```

## ⚙️ 配置指南

### 数据库

数据库配置在第 02 步的「配置目标数据库」面板填写，字段对应后端 `DatabaseConfig`：

| 字段 | 必填 | 默认 |
| --- | --- | --- |
| `db_type` | 是 | `mysql` |
| `host` | 是 | `127.0.0.1` |
| `port` | 是 | 按 `db_type` 自动填充（3306 / 5432 / 1521 / 5236） |
| `database` | 是 | — |
| `username` | 是 | — |
| `password` | 是 | — |
| `schema` | 否（仅 PostgreSQL 系列） | — |

工具内部使用 `MySqlConnectOptions` / `PgConnectOptions` 构造连接，无需手动 URL 编码密码中的特殊字符（`@` / `:` / `/` 等）。

### LLM 智能识别

兼容 OpenAI Chat Completions 协议的服务均可使用，仅需配置 API URL（base URL）、API Key、模型名称：

| 服务 | API URL 示例 | 推荐模型 |
| --- | --- | --- |
| OpenAI | `https://api.openai.com/v1` | `gpt-4o-mini` |
| DeepSeek | `https://api.deepseek.com/v1` | `deepseek-chat` |
| 通义千问 | `https://dashscope.aliyuncs.com/compatible-mode/v1` | `qwen-plus` |

实际请求地址由工具自动拼接为 `{api_url}/chat/completions`，默认 30 秒超时。

### Tauri 权限与窗口

- 窗口配置：`src-tauri/tauri.conf.json`
- 权限模型：`src-tauri/capabilities/default.json`（已声明 dialog / shell / fs / 核心窗口能力）

### 达梦 DM 驱动打包

达梦 DM 适配器使用 ODBC 协议，驱动二进制文件随应用安装包一并分发，最终用户无需手动安装。开发者构建发布版前需将驱动文件放入对应目录：

```bash
# 查看打包说明
make bundle-drivers
```

驱动文件放置位置：

| 平台 | 目标路径 |
| --- | --- |
| macOS | `src-tauri/bundled-drivers/dm-odbc/macos/libdmodbc.dylib` |
| Linux | `src-tauri/bundled-drivers/dm-odbc/linux/libdmodbc.so` |
| Windows | `src-tauri/bundled-drivers/dm-odbc/windows/dmodbc.dll` |

如构建时未提供驱动文件，应用运行时会回退使用系统已安装的 DM ODBC 驱动。

Oracle 适配器使用 oracle-rs 纯 Rust TNS 协议实现，无需任何额外驱动文件。

## 🧭 使用流程

工具采用 5 步线性向导：

1. **选择源文件目录**：选择含 `.flg` 与 `.dat.gz` 的目录，自动扫描并表格化展示文件对
2. **配置目标数据库**：选择数据库类型、填写连接参数，可展开 LLM 面板从自然语言中识别填充
3. **前置检查**：一键运行磁盘空间 / 文件格式 / 一致性检查，error 项阻塞下一步
4. **执行加载**：实时展示总进度与表级进度、终端风格日志面板、概览统计四联卡，可中途取消
5. **查看报告**：汇总卡片、每表详情、速率柱状图、成功/失败饼图，一键导出 JSON

`example_data/` 下提供 5 组合计 12300 行的样例数据（employee / order / product / transaction / user），可直接作为第 01 步的输入。

## 🗄️ 支持的数据库

| 数据库 | 协议 | 默认端口 | 状态 |
| --- | --- | --- | --- |
| MySQL | MySQL | 3306 | ✅ 完整支持 |
| TXSQL | MySQL 兼容 | 3306 | ✅ 完整支持 |
| TDSQL | MySQL 兼容 | 3306 | ✅ 完整支持 |
| PostgreSQL | PostgreSQL | 5432 | ✅ 完整支持 |
| openGauss | PG 兼容 | 5432 | ✅ 完整支持 |
| GaussDB | PG 兼容 | 5432 | ✅ 完整支持 |
| Oracle | TNS | 1521 | ✅ 完整支持 |
| 达梦 DM | ODBC | 5236 | ✅ 完整支持 |

## 📁 项目结构

```
gut-loader/
├── docs/                  # PRD / 实现文档 / 变更记录
├── example_data/          # 5 组样例数据
├── frontend/              # 前端源码（React + TypeScript）
│   ├── src/
│   │   ├── components/    # 业务组件 + shadcn/ui
│   │   ├── hooks/         # IPC 与事件订阅钩子
│   │   ├── pages/         # 向导页面
│   │   ├── stores/        # Zustand 全局 store
│   │   └── lib/           # 类型定义与工具
│   ├── index.html
│   ├── package.json
│   ├── vite.config.ts
│   └── tsconfig.json
├── src-tauri/             # 后端源码（Rust + Tauri）
│   ├── src/parser/        # FLG / DAT 解析、目录扫描
│   ├── src/validator/     # 磁盘 / 文件前置检查
│   ├── src/database/      # MySQL / PostgreSQL / Oracle / DM 适配器
│   ├── src/llm/           # OpenAI 兼容客户端
│   ├── src/loader/        # 单表加载编排
│   ├── src/report/        # 报告聚合、JSON / 摘要导出
│   ├── src/commands/      # Tauri 命令与 AppState
│   ├── src/models.rs      # 跨模块共享数据结构
│   └── tests/             # 集成测试
├── Makefile               # 统一构建入口
└── README.md
```

## 🛠️ 开发指南

### 前端

- 前端代码位于 `frontend/` 子目录
- 路径别名 `@/*` 指向 `frontend/src/*`
- 组件遵循 shadcn/ui new-york 风格、slate 基础色，新增组件请同时更新 `frontend/components.json`
- 全局状态使用 Zustand store，通过反选器 hook 按需订阅以避免不必要的重渲染
- 所有 IPC 调用集中在 `frontend/src/hooks/useTauriCommands.ts`，事件订阅集中在 `frontend/src/hooks/useLoadingEvents.ts`，组件层不直接调用 `@tauri-apps/api`

### 后端

- 模块按职责拆分：`parser` / `validator` / `database` / `llm` / `loader` / `report` / `commands` / `models`
- 异常处理使用 `anyhow::Result` + `thiserror`，跨边界返回 `Result<T, String>` 以便前端展示
- 日志使用 `tracing`，按 target 区分模块（如 `target = "loader"`）
- 新增数据库适配器需实现 `DatabaseLoader` trait 并在 `create_loader` 工厂中路由

### 类型对齐

前端 `frontend/src/lib/types.ts` 与后端 `src-tauri/src/models.rs` 字段一一对应。修改任意一侧时务必同步另一侧，避免 IPC 反序列化失败。

## ❓ FAQ

**Q：`cargo check` 提示缺少图标？**
当前仓库已生成最小占位图标（`src-tauri/icons/`），如需替换为正式图标，覆盖同名文件即可。

**Q：是否需要单独安装 Tauri CLI？**
不需要。`@tauri-apps/cli` 已作为 `frontend/package.json` 的 devDependency 提供，Makefile 通过 `./frontend/node_modules/.bin/tauri` 调用。

**Q：测试数据在哪里？**
项目根目录 `example_data/` 已提供 5 组测试样例（dat.gz + flg），合计 12300 行，可直接作为第 01 步的输入。

**Q：加载过程中能否中途取消？**
点击「停止加载」按钮会设置取消标志，后台任务在当前表完成后退出主循环；当前表内不会被中断，已写入数据保持完整。

**Q：是否支持断点续传？**
支持。基于 `SELECT COUNT(*)` 取得目标表已有行数，跳过解析结果中的对应前缀。前提是数据按追加模式写入，不删除、不更新。

**Q：LLM 识别失败或返回乱码怎么办？**
工具内置响应解析降级：纯 JSON 反序列化失败时会从响应中提取首个花括号配平的 JSON 片段；仍失败则返回带默认值的 `DatabaseConfig`，请人工核对后再继续。

**Q：数据库密码包含 `@` / `:` 等特殊字符是否需要转义？**
不需要。工具内部使用 `MySqlConnectOptions` / `PgConnectOptions` 结构体构造连接，避免了 URL 编码问题。

**Q：PostgreSQL 报「inconsistent types deduced for parameter」？**
该问题已在 v0.7.1 修复：批量插入占位符上会附加 `::TEXT` / `::NUMERIC` / `::BIGINT` 显式类型转换后缀，并将数值列空字符串绑定为 `NULL`。

**Q：构建产物在哪里？**
`make build` 完成后，构建产物位于 `src-tauri/target/release/bundle/` 下，按平台输出对应安装包。

## 📜 许可证

内部项目 / TBD。
