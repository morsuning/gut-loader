# GUT 数据加载工具 - 构建脚本
# 支持多平台构建、本地开发调试和常用操作

.DEFAULT_GOAL := help

# Tauri CLI 路径（从 frontend/node_modules 获取，运行于项目根目录以定位 src-tauri）
TAURI_BIN := ./frontend/node_modules/.bin/tauri

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

# 构建macOS arm64
build-macos-arm:
	$(TAURI_BIN) build --target aarch64-apple-darwin

# 构建Windows x64（需要Windows或已配置交叉编译环境）
build-windows-x64:
	$(TAURI_BIN) build --target x86_64-pc-windows-msvc

# 构建Linux x64（需要Linux或已配置交叉编译环境）
build-linux-x64:
	$(TAURI_BIN) build --target x86_64-unknown-linux-gnu

# 构建Linux arm64（需要Linux或已配置交叉编译环境）
build-linux-arm:
	$(TAURI_BIN) build --target aarch64-unknown-linux-gnu

# 构建所有声明平台（需在对应OS或CI矩阵中执行）
build-all: build-macos-arm build-windows-x64 build-linux-x64 build-linux-arm
	@echo "构建完成。跨平台构建需要在对应OS上执行或使用CI。"

# ==========================================
# 清理
# ==========================================

# 清理构建产物
clean:
	rm -rf frontend/node_modules
	rm -rf frontend/dist
	cd src-tauri && cargo clean
	rm -rf src-tauri/target

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
	@echo "  make build            构建当前平台"
	@echo "  make build-macos-arm  构建macOS ARM64"
	@echo "  make build-windows-x64 构建Windows x64(需对应环境)"
	@echo "  make build-linux-x64  构建Linux x64(需对应环境)"
	@echo "  make build-linux-arm  构建Linux arm64(需对应环境)"
	@echo "  make build-all        构建全部声明平台(需CI矩阵或交叉编译环境)"
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
	@echo "  make clean-rust       仅清理Rust缓存"
	@echo "  make rebuild          清理并重新构建"

.PHONY: install dev dev-web check test test-integration lint \
	build build-macos build-macos-arm build-windows build-windows-x64 build-linux build-linux-x64 build-linux-arm build-all \
	bundle-drivers \
	clean clean-rust rebuild \
	db-up db-down db-status \
	help
