# 构建文档

本文档说明如何在 macOS 上交叉编译 Windows x64、Linux x64 和 Linux arm64 版本。

## 1. 适用范围

该构建方式面向 macOS 开发机，目标是在本机直接产出以下发布物：

- Windows x64：`x86_64-pc-windows-gnu`
- Linux x64：`x86_64-unknown-linux-gnu`
- Linux arm64：`aarch64-unknown-linux-gnu`

macOS 本机仍可正常构建当前平台应用。

## 2. 前置条件

### 2.1 基础工具

- macOS 终端环境
- Node.js 18+
- Rust stable
- npm
- Docker Desktop

### 2.2 Windows x64 交叉编译

需要安装：

- `mingw-w64`
- `makensis`

建议命令：

```bash
brew install mingw-w64 makensis
rustup target add x86_64-pc-windows-gnu
```

### 2.3 Linux x64 / arm64 交叉编译

需要安装：

- Docker Desktop

构建过程会在 Linux 容器中完成，容器内安装 Rust、Node.js 以及 Tauri 所需系统依赖。

## 3. 构建命令

### 3.1 Windows x64

```bash
make build-windows-x64
```

产物位置：

- `src-tauri/target/x86_64-pc-windows-gnu/release/gut-loader.exe`
- `src-tauri/target/x86_64-pc-windows-gnu/release/bundle/nsis/*.exe`

### 3.2 Linux x64

```bash
make build-linux-x64
```

产物位置：

- `src-tauri/target/x86_64-unknown-linux-gnu/release/`

### 3.3 Linux arm64

```bash
make build-linux-arm
```

产物位置：

- `src-tauri/target/aarch64-unknown-linux-gnu/release/`

### 3.4 全量构建

```bash
make build-all
```

该命令按顺序执行 macOS arm64、Windows x64、Linux x64、Linux arm64 构建。

## 4. 实现说明

### 4.1 Windows x64

macOS 上不使用 MSVC 直接交叉链接，而是采用 GNU 工具链：

- Rust target：`x86_64-pc-windows-gnu`
- 链接器：`x86_64-w64-mingw32-gcc`
- 安装器：`makensis`

这种方式可以在 macOS 上直接生成 Windows 可执行文件和 NSIS 安装包。

### 4.2 Linux x64 / arm64

macOS 上通过 Docker 构建 Linux 目标，容器内执行：

- 安装 Rust 工具链
- 安装 Node.js
- 安装 Tauri 所需 Linux 系统库
- 执行前端构建与 Tauri 打包

Linux x64 使用 `linux/amd64` 容器平台，Linux arm64 使用 `linux/arm64` 容器平台。

## 5. 注意事项

- Windows x64 在 macOS 上生成的是 GNU 目标，不是 MSVC 目标。
- Linux 构建依赖 Docker 下载基础镜像和系统包，首次构建耗时较长。
- 构建过程中会重新生成前端 `dist/` 与后端 `target/` 目录内容。
- 若本机 Cargo 源配置指向不可用镜像源，需要先恢复可用的 crates.io 配置。
