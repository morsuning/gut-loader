# 构建文档

本文档说明如何构建 GUT 数据加载工具的发布版本。所有构建命令都会使用 Tauri release 构建链路，Rust release profile 已显式开启 `opt-level = 3`、fat LTO、单 codegen unit、符号裁剪与 `panic = "abort"`。

## 1. 适用范围

项目根目录 `Makefile` 支持以下发布目标：

| 平台 | Rust target | 说明 |
| --- | --- | --- |
| macOS Apple Silicon | `aarch64-apple-darwin` | macOS 仅支持 Apple Silicon 构建，不支持达梦数据库入口 |
| Windows x64 | `x86_64-pc-windows-gnu` | macOS 开发机使用 mingw-w64 交叉编译 |
| Linux x64 | `x86_64-unknown-linux-gnu` | macOS 使用 Docker 构建，Linux 可直接本地构建 |
| Linux arm64 | `aarch64-unknown-linux-gnu` | macOS 使用 Docker arm64 镜像构建，Linux 可直接本地构建 |

所有构建产物都会统一复制到项目根目录 `dist/`，不再按平台拆分子目录。`src-tauri/target/` 仍保留 Tauri 和 Cargo 的原始中间产物。

## 2. 前置条件

### 2.1 基础工具

- macOS 终端环境
- Node.js 18+
- Rust stable
- npm
- Tauri v2 所需系统依赖

建议先执行：

```bash
make install
```

### 2.2 Rust targets

```bash
rustup target add aarch64-apple-darwin
rustup target add x86_64-pc-windows-gnu
```

Makefile 中的各平台构建目标也会在执行前调用对应 `rustup target add`，但提前安装可以更早暴露网络或工具链问题。

### 2.3 Windows x64 交叉编译

需要安装：

- `mingw-w64`
- `makensis`

建议命令：

```bash
brew install mingw-w64 makensis
```

构建时 Makefile 会使用：

- `x86_64-w64-mingw32-gcc`
- `x86_64-pc-windows-gnu`

### 2.4 Linux x64 / arm64 交叉编译

在 macOS 上构建 Linux 目标必须使用 Docker Linux 环境，因为 Tauri v2 在 Linux 上依赖 GTK3/WebKitGTK 等系统库，这些库需要在真实 Linux 发行版环境中由 `pkg-config` 发现并参与链接。

需要安装：

- Docker Desktop 或 Docker Engine

构建镜像 `Dockerfile.linux-build` 会在首次执行时自动构建，基于 Ubuntu 24.04 提供完整的 Tauri v2 Linux 构建环境（包括 `libgtk-3-dev`、`libwebkit2gtk-4.1-dev`、`libjavascriptcoregtk-4.1-dev` 等系统依赖）。Tauri 2 的 Linux WebView 依赖需要 `javascriptcoregtk-4.1.pc`，因此镜像必须提供 WebKitGTK 4.1，而不是 Ubuntu 22.04 中常见的 WebKitGTK 4.0。

- Linux x64：通过 `--platform linux/amd64` 在 Docker 中构建，目标为 `x86_64-unknown-linux-gnu`
- Linux arm64：通过 `--platform linux/arm64` 在 Docker 中构建，目标为 `aarch64-unknown-linux-gnu`

在 Linux 系统上可直接本地构建，无需 Docker。

Linux 目标统一使用 GNU libc 目标，因为 Tauri 应用的 GTK/WebKit 依赖需要动态链接 Linux 系统库。

## 3. 构建命令

### 3.1 当前平台

```bash
make build
```

该命令执行当前平台 release 构建，并将发布产物复制到 `dist/`。

### 3.2 macOS Apple Silicon

```bash
make build-macos-arm
```

### 3.3 Windows x64

```bash
make build-windows-x64
```

### 3.4 Linux x64

```bash
make build-linux-x64
```

### 3.5 Linux arm64

```bash
make build-linux-arm
```

### 3.6 全量构建

```bash
make build-all
```

该命令会先清空并重建 `dist/`，再按顺序执行：

1. `make build-macos-arm`
2. `make build-windows-x64`
3. `make build-linux-x64`
4. `make build-linux-arm`

## 4. 产物位置

所有发布产物统一位于：

```text
dist/
```

Makefile 会从各 target 的 release 输出中归集可执行文件与安装包，包括：

- macOS：`.dmg`、`.app.tar.gz`
- Windows：`.exe`、`.msi`
- Linux：可执行文件、`.deb`、`.rpm`、`.AppImage`

为避免同名裸二进制在平铺目录中互相覆盖，`gut-loader` / `gut-loader.exe` 会复制为带平台后缀的文件名，例如 `gut-loader-linux-x64`、`gut-loader-linux-arm64`、`gut-loader-x86_64-pc-windows-gnu.exe`。安装包文件保持 Tauri 原始文件名。

如果 Tauri 对某个平台生成了额外中间文件，仍会保留在 `src-tauri/target/`，不会复制到 `dist/`。

## 5. 清理命令

```bash
make clean-dist
```

仅清理并重建 `dist/`。

```bash
make clean
```

清理前端依赖、前端构建产物、Cargo target 与 `dist/`。

## 6. 注意事项

- `make build-all` 是发布矩阵入口，产物只需要从项目根目录 `dist/` 获取。
- Linux 交叉编译在 macOS 上依赖 Docker；缺少 Docker 时 Makefile 会因 `docker` 命令失败而报错。在 Linux 系统上可直接本地构建。
- Windows x64 在 macOS 上生成的是 GNU 目标，不是 MSVC 目标。
- 达梦数据库仅在 Windows x64、Linux x64 与 Linux arm64 支持平台可用；macOS 版本不会展示达梦数据库入口。
- 若本机 Cargo 源配置指向不可用镜像源，需要先恢复可用的 crates.io 配置。
