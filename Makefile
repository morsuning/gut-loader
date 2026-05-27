# GUT 数据加载工具 - 构建脚本
# 支持多平台构建、本地开发调试和常用操作

.DEFAULT_GOAL := help

# Tauri CLI 路径（从 frontend/node_modules 获取，运行于项目根目录以定位 src-tauri）
TAURI_BIN := ./frontend/node_modules/.bin/tauri
UNAME_S := $(shell uname -s)
DIST_DIR := dist
APP_NAME := gut-loader

WINDOWS_MSVC_TARGET := x86_64-pc-windows-msvc
WINDOWS_GNU_TARGET := x86_64-pc-windows-gnu
LINUX_X64_TARGET := x86_64-unknown-linux-gnu
LINUX_ARM_TARGET := aarch64-unknown-linux-gnu
LINUX_BUILDER_IMAGE := gut-loader-linux-builder

define collect_release
	@mkdir -p $(DIST_DIR)
	@echo "归集 $(1) 发布产物到 $(DIST_DIR)/"
	@find src-tauri/target/$(1)/release/bundle \
		-maxdepth 3 -type f \
		\( -name '*.dmg' -o -name '*.app.tar.gz' -o -name '*.app.tar.gz.sig' -o -name '*.msi' -o -name '*.deb' -o -name '*.rpm' -o -name '*.AppImage' -o \( -path '*/bundle/*' -name '*.exe' \) \) \
		-not -path '*/deps/*' \
		-exec cp -f {} $(DIST_DIR)/ \; 2>/dev/null || true
endef

define collect_host_release
	@mkdir -p $(DIST_DIR)
	@echo "归集当前平台发布产物到 $(DIST_DIR)/"
	@find src-tauri/target/release/bundle \
		-maxdepth 3 -type f \
		\( -name '*.dmg' -o -name '*.app.tar.gz' -o -name '*.app.tar.gz.sig' -o -name '*.msi' -o -name '*.deb' -o -name '*.rpm' -o -name '*.AppImage' -o \( -path '*/bundle/*' -name '*.exe' \) \) \
		-not -path '*/deps/*' \
		-exec cp -f {} $(DIST_DIR)/ \; 2>/dev/null || true
endef

# ==========================================
# 开发调试
# ==========================================

# 安装所有依赖
install:
	cd frontend && npm install
	cd src-tauri && cargo fetch

# 本地开发运行（热重载）
dev:
	$(TAURI_BIN) dev

# 仅运行前端开发服务器
dev-web:
	cd frontend && npm run dev

# 仅检查Rust编译
check:
	cd src-tauri && cargo check

# 运行所有测试
test:
	cd src-tauri && cargo test

# 运行集成测试（需要Docker数据库）
test-integration:
	docker run -d --name gut-test-mysql -e MYSQL_ROOT_PASSWORD=testpass123 -e MYSQL_DATABASE=gut_test -p 3307:3306 mysql:8.0 || true
	docker run -d --name gut-test-postgres -e POSTGRES_PASSWORD=testpass123 -e POSTGRES_DB=gut_test -p 5433:5432 postgres:16 || true
	@echo "等待数据库启动..."
	@sleep 15
	cd src-tauri && cargo test --test integration_test -- --nocapture
	docker stop gut-test-mysql gut-test-postgres || true
	docker rm gut-test-mysql gut-test-postgres || true

# 代码检查
lint:
	cd src-tauri && cargo clippy -- -D warnings
	cd frontend && npx tsc --noEmit

# ==========================================
# 构建打包
# ==========================================

# 构建当前平台
build:
	$(TAURI_BIN) build
	$(call collect_host_release)

# 构建macOS arm64
build-macos-arm:
	$(TAURI_BIN) build --target aarch64-apple-darwin
	$(call collect_release,aarch64-apple-darwin)

# 构建Windows x64；macOS 使用 GNU 工具链交叉编译，其他平台保留 MSVC 目标
# 构建前清理 NSIS bundle 目录中的旧版本安装包，避免 dist/ 中出现多个版本产物
build-windows-x64:
ifeq ($(UNAME_S),Darwin)
	@command -v x86_64-w64-mingw32-gcc >/dev/null || (echo "缺少 mingw-w64：brew install mingw-w64" && exit 1)
	@command -v makensis >/dev/null || (echo "缺少 NSIS：brew install makensis" && exit 1)
	rustup target add $(WINDOWS_GNU_TARGET)
	@rm -rf src-tauri/target/$(WINDOWS_GNU_TARGET)/release/bundle/nsis/*.exe
	PATH="/opt/homebrew/bin:$(PATH)" CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=x86_64-w64-mingw32-gcc $(TAURI_BIN) build --target $(WINDOWS_GNU_TARGET)
	$(call collect_release,$(WINDOWS_GNU_TARGET))
else
	@rm -rf src-tauri/target/$(WINDOWS_MSVC_TARGET)/release/bundle/nsis/*.exe
	$(TAURI_BIN) build --target $(WINDOWS_MSVC_TARGET)
	$(call collect_release,$(WINDOWS_MSVC_TARGET))
endif

# 构建Linux x64；macOS 使用 Docker 构建，Linux 可直接本地构建
# 构建前清理 bundle 目录中的旧版本产物，避免 dist/ 中出现多个版本
build-linux-x64:
ifeq ($(UNAME_S),Darwin)
	@echo "=== macOS 上使用 Docker 构建 Linux x64 ==="
	@rm -rf src-tauri/target/$(LINUX_X64_TARGET)/release/bundle/deb/*.deb src-tauri/target/$(LINUX_X64_TARGET)/release/bundle/rpm/*.rpm src-tauri/target/$(LINUX_X64_TARGET)/release/bundle/appimage/*.AppImage
	@docker build --platform linux/amd64 -t $(LINUX_BUILDER_IMAGE) -f Dockerfile.linux-build .
	@docker run --rm --platform linux/amd64 \
		-v "$(PWD):/app" \
		$(LINUX_BUILDER_IMAGE) \
		sh -c "cd /app/frontend && npm ci && cd /app && ./frontend/node_modules/.bin/tauri build --target $(LINUX_X64_TARGET)"
	$(call collect_release,$(LINUX_X64_TARGET))
	@echo "=== Linux x64 发布产物已输出到 $(DIST_DIR)/ ==="
else
	@rm -rf src-tauri/target/$(LINUX_X64_TARGET)/release/bundle/deb/*.deb src-tauri/target/$(LINUX_X64_TARGET)/release/bundle/rpm/*.rpm src-tauri/target/$(LINUX_X64_TARGET)/release/bundle/appimage/*.AppImage
	rustup target add $(LINUX_X64_TARGET)
	$(TAURI_BIN) build --target $(LINUX_X64_TARGET)
	$(call collect_release,$(LINUX_X64_TARGET))
endif

# 构建Linux arm64；macOS(Apple Silicon)使用 Docker arm64 镜像构建，Linux 可直接本地构建
# 构建前清理 bundle 目录中的旧版本产物，避免 dist/ 中出现多个版本
build-linux-arm:
ifeq ($(UNAME_S),Darwin)
	@echo "=== macOS 上使用 Docker 构建 Linux arm64 ==="
	@rm -rf src-tauri/target/$(LINUX_ARM_TARGET)/release/bundle/deb/*.deb src-tauri/target/$(LINUX_ARM_TARGET)/release/bundle/rpm/*.rpm src-tauri/target/$(LINUX_ARM_TARGET)/release/bundle/appimage/*.AppImage
	@docker build -t $(LINUX_BUILDER_IMAGE)-arm64 -f Dockerfile.linux-build --platform linux/arm64 .
	@docker run --rm --platform linux/arm64 \
		-v "$(PWD):/app" \
		$(LINUX_BUILDER_IMAGE)-arm64 \
		sh -c "cd /app/frontend && npm ci && cd /app && ./frontend/node_modules/.bin/tauri build --target $(LINUX_ARM_TARGET)"
	$(call collect_release,$(LINUX_ARM_TARGET))
	@echo "=== Linux arm64 发布产物已输出到 $(DIST_DIR)/ ==="
else
	@rm -rf src-tauri/target/$(LINUX_ARM_TARGET)/release/bundle/deb/*.deb src-tauri/target/$(LINUX_ARM_TARGET)/release/bundle/rpm/*.rpm src-tauri/target/$(LINUX_ARM_TARGET)/release/bundle/appimage/*.AppImage
	rustup target add $(LINUX_ARM_TARGET)
	$(TAURI_BIN) build --target $(LINUX_ARM_TARGET)
	$(call collect_release,$(LINUX_ARM_TARGET))
endif

# 平台别名
build-macos: build-macos-arm

build-windows: build-windows-x64

build-linux: build-linux-x64

# 构建所有声明平台
build-all: clean-dist build-macos-arm build-windows-x64 build-linux-x64 build-linux-arm
	@echo "构建完成。所有发布产物已统一归集到 $(DIST_DIR)/。"

clean-dist:
	rm -rf $(DIST_DIR)
	mkdir -p $(DIST_DIR)

# ==========================================
# 清理
# ==========================================

# 清理构建产物
clean:
	rm -rf frontend/node_modules
	rm -rf frontend/dist
	cd src-tauri && cargo clean
	rm -rf src-tauri/target
	rm -rf $(DIST_DIR)

# 清理并重新安装
rebuild: clean install

# 仅清理Rust构建缓存
clean-rust:
	cd src-tauri && cargo clean

# ==========================================
# 驱动打包
# ==========================================

## 驱动打包提示
bundle-drivers:
	@echo "=== 达梦 DM ODBC 驱动打包 ==="
	@echo "请从达梦官方 DM8 安装介质中提取 ODBC 驱动文件。"
	@echo "macOS 版本不支持达梦数据库。"
	@echo "将 ODBC 驱动文件放入以下目录："
	@echo "  Windows x64:   src-tauri/bundled-drivers/dm-odbc/windows/x64/dmodbc.dll"
	@echo "  Linux x64:     src-tauri/bundled-drivers/dm-odbc/linux/x64/libdmodbc.so"
	@echo "  Linux arm64:   src-tauri/bundled-drivers/dm-odbc/linux/arm64/libdmodbc.so"
	@echo ""
	@echo "Oracle 使用纯 Rust 驱动，无需额外文件。"

# ==========================================
# Docker数据库管理（用于测试）
# ==========================================

# 启动测试数据库
db-up:
	docker run -d --name gut-test-mysql -e MYSQL_ROOT_PASSWORD=testpass123 -e MYSQL_DATABASE=gut_test -p 3307:3306 mysql:8.0 || true
	docker run -d --name gut-test-postgres -e POSTGRES_PASSWORD=testpass123 -e POSTGRES_DB=gut_test -p 5433:5432 postgres:16 || true
	@echo "数据库启动中，请等待约15秒..."

# 停止测试数据库
db-down:
	docker stop gut-test-mysql gut-test-postgres 2>/dev/null || true
	docker rm gut-test-mysql gut-test-postgres 2>/dev/null || true

# 查看数据库状态
db-status:
	@docker ps --filter "name=gut-test" --format "table {{.Names}}\t{{.Status}}\t{{.Ports}}"

# ==========================================
# 帮助
# ==========================================

help:
	@echo "GUT 数据加载工具 - 构建命令"
	@echo ""
	@echo "开发调试:"
	@echo "  make install          安装所有依赖"
	@echo "  make dev              本地开发运行(热重载)"
	@echo "  make dev-web          仅运行前端开发服务器"
	@echo "  make check            Rust编译检查"
	@echo "  make test             运行单元测试"
	@echo "  make test-integration 运行集成测试(需Docker)"
	@echo "  make lint             代码质量检查"
	@echo ""
	@echo "构建打包:"
	@echo "  make build            构建当前平台 release 版本并归集到 dist/"
	@echo "  make build-macos-arm  构建macOS ARM64 release 版本"
	@echo "  make build-windows-x64 构建Windows x64(macOS下使用mingw-w64)"
	@echo "  make build-linux-x64  构建Linux GNU x64(macOS下使用Docker)"
	@echo "  make build-linux-arm  构建Linux GNU arm64(macOS下使用Docker)"
	@echo "  make build-all        构建全部声明平台并统一输出到 dist/"
	@echo ""
	@echo "驱动打包:"
	@echo "  make bundle-drivers   显示达梦ODBC驱动打包说明"
	@echo ""
	@echo "数据库管理:"
	@echo "  make db-up            启动测试数据库(Docker)"
	@echo "  make db-down          停止测试数据库"
	@echo "  make db-status        查看数据库状态"
	@echo ""
	@echo "清理:"
	@echo "  make clean            清理所有构建产物"
	@echo "  make clean-dist       仅清理 dist/ 归集目录"
	@echo "  make clean-rust       仅清理Rust缓存"
	@echo "  make rebuild          清理并重新构建"

.PHONY: install dev dev-web check test test-integration lint \
	build build-macos build-macos-arm build-windows build-windows-x64 build-linux build-linux-x64 build-linux-arm build-all \
	bundle-drivers \
	clean clean-dist clean-rust rebuild \
	db-up db-down db-status \
	help
