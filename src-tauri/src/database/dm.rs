//! 达梦 DM 数据库适配器：基于 ODBC（odbc-api）实现，运行时自动发现打包的驱动。
//!
//! 设计要点：
//! 1. odbc-api 的 Connection 不是 Send + Sync，因此持有连接字符串，每次操作在
//!    `tokio::task::spawn_blocking` 内创建独立的 Environment + Connection。
//! 2. 启动时通过 `resolve_driver_path` 自动定位打包目录中的达梦 ODBC 驱动文件，
//!    避免要求最终用户安装系统级驱动；定位失败时回退到 `DM8 ODBC DRIVER` 名称，
//!    这要求系统已注册同名驱动。
//! 3. 仅在 Windows x64、Linux x64 与 Linux arm64 目标上编译，macOS 不支持达梦。

use super::DatabaseLoader;
use crate::database::safe_batch_size;
use crate::models::{ColumnDefinition, ColumnType, DataRow, DatabaseConfig, FlgMetadata};
use anyhow::{Context, Result};
use async_trait::async_trait;
use odbc_api::{ConnectionOptions, Cursor, Environment, IntoParameter};
use std::path::PathBuf;
use tracing::{info, warn};

/// 达梦加载器
///
/// 持有 ODBC 连接字符串，每次操作在 `spawn_blocking` 内独立创建连接。
pub struct DmLoader {
    connection_string: String,
}

/// 解析打包的达梦 ODBC 驱动路径。
///
/// 查找顺序：
/// 1. 生产环境（Tauri 资源目录）：根据可执行文件路径推断。
///    - Linux：`<exe_dir>/bundled-drivers/dm-odbc/linux/<arch>/`
///    - Windows：`<exe_dir>\bundled-drivers\dm-odbc\windows\x64\`
/// 2. 开发模式回退：`CARGO_MANIFEST_DIR/bundled-drivers/dm-odbc/<platform>/<arch>/`。
/// 3. 兼容旧目录：`<platform>/` 直放驱动文件的老结构。
///
/// 都未找到时返回 `None`，调用方应回退使用系统注册的驱动名 `DM8 ODBC DRIVER`。
fn resolve_driver_path() -> Option<PathBuf> {
    let driver_file = if cfg!(target_os = "windows") {
        "dmodbc.dll"
    } else {
        "libdmodbc.so"
    };

    let platform_subdir = if cfg!(target_os = "windows") {
        "windows"
    } else {
        "linux"
    };
    let arch_subdir = if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        "x64"
    };

    // 优先级 1：基于可执行文件位置定位资源目录（生产环境）
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let platform_dir = if cfg!(target_os = "windows") {
                exe_dir
                    .join("bundled-drivers/dm-odbc")
                    .join(platform_subdir)
                    .join(arch_subdir)
            } else {
                exe_dir
                    .join("bundled-drivers/dm-odbc")
                    .join(platform_subdir)
                    .join(arch_subdir)
            };

            let driver_path = platform_dir.join(driver_file);
            if driver_path.exists() {
                return Some(driver_path);
            }

            let legacy_driver_path = platform_dir
                .parent()
                .map(|dir| dir.join(driver_file))
                .filter(|path| path.exists());
            if legacy_driver_path.is_some() {
                return legacy_driver_path;
            }
        }
    }

    // 优先级 2：开发模式从项目目录查找
    let dev_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("bundled-drivers/dm-odbc")
        .join(platform_subdir)
        .join(arch_subdir)
        .join(driver_file);
    if dev_path.exists() {
        Some(dev_path)
    } else {
        let legacy_dev_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("bundled-drivers/dm-odbc")
            .join(platform_subdir)
            .join(driver_file);
        if legacy_dev_path.exists() {
            Some(legacy_dev_path)
        } else {
            None
        }
    }
}

/// 根据 ColumnType 生成达梦 DDL 类型字符串。
///
/// 委托给 `crate::database::column_type_to_dm_ddl`，避免逻辑分散。
fn column_type_to_dm_ddl(col: &ColumnDefinition) -> String {
    match &col.data_type {
        ColumnType::Varchar(n) => format!("VARCHAR({})", n),
        ColumnType::Decimal(m, n) => format!("DECIMAL({},{})", m, n),
        ColumnType::Int(_) => "BIGINT".to_string(),
    }
}

impl DmLoader {
    /// 根据配置创建达梦加载器。
    ///
    /// 流程：
    /// 1. 通过 `resolve_driver_path` 自动发现打包驱动；找不到时回退到驱动名。
    /// 2. 构建 ODBC 连接字符串，密码中的 `}` 转义为 `}}`。
    /// 3. 在 `spawn_blocking` 内执行一次连接验证以提前暴露驱动/网络/认证问题。
    pub async fn new(config: &DatabaseConfig) -> Result<Self> {
        let driver_spec = match resolve_driver_path() {
            Some(path) => {
                info!("发现打包达梦 ODBC 驱动: {}", path.display());
                path.to_string_lossy().to_string()
            }
            None => {
                warn!("未发现打包达梦 ODBC 驱动，回退到系统注册的 DM8 ODBC DRIVER");
                "DM8 ODBC DRIVER".to_string()
            }
        };

        // 密码中 } 需要转义为 }}，并整体用 {} 包裹以兼容含特殊字符的密码
        let escaped_pwd = config.password.replace('}', "}}");
        let connection_string = format!(
            "Driver={{{}}};Server={};TCP_PORT={};DATABASE={};UID={};PWD={{{}}}",
            driver_spec, config.host, config.port, config.database, config.username, escaped_pwd
        );

        info!(
            "连接达梦数据库: {}:{}/{}",
            config.host, config.port, config.database
        );

        // 测试连接验证驱动可用
        let conn_str = connection_string.clone();
        let host = config.host.clone();
        let port = config.port;
        let database = config.database.clone();
        tokio::task::spawn_blocking(move || -> Result<()> {
            let env =
                Environment::new().map_err(|e| anyhow::anyhow!("创建 ODBC 环境失败: {}", e))?;
            let conn = env
                .connect_with_connection_string(&conn_str, ConnectionOptions::default())
                .map_err(|e| anyhow::anyhow!("连接达梦数据库失败: {}", e))?;
            conn.execute("SELECT 1", ())
                .map_err(|e| anyhow::anyhow!("测试查询失败: {}", e))?;
            Ok(())
        })
        .await
        .with_context(|| "spawn_blocking 执行失败")?
        .with_context(|| format!("连接达梦数据库失败: {}:{}/{}", host, port, database))?;

        info!(
            "达梦 DM 连接成功: {}:{}/{}",
            config.host, config.port, config.database
        );
        Ok(Self { connection_string })
    }

    /// 在 spawn_blocking 内创建连接并执行闭包。
    ///
    /// 每次调用都会创建新的 ODBC Environment 和 Connection（ODBC 连接不是 Send）。
    async fn with_connection<F, T>(&self, f: F) -> Result<T>
    where
        F: FnOnce(odbc_api::Connection<'_>) -> Result<T> + Send + 'static,
        T: Send + 'static,
    {
        let conn_str = self.connection_string.clone();
        tokio::task::spawn_blocking(move || -> Result<T> {
            let env =
                Environment::new().map_err(|e| anyhow::anyhow!("创建 ODBC 环境失败: {}", e))?;
            let conn = env
                .connect_with_connection_string(&conn_str, ConnectionOptions::default())
                .map_err(|e| anyhow::anyhow!("连接达梦数据库失败: {}", e))?;
            f(conn)
        })
        .await
        .with_context(|| "spawn_blocking 执行失败")?
    }
}

#[async_trait]
impl DatabaseLoader for DmLoader {
    /// 测试数据库连接是否可用。
    async fn test_connection(&self) -> Result<bool> {
        self.with_connection(|conn| {
            conn.execute("SELECT 1", ())
                .map_err(|e| anyhow::anyhow!("测试连接失败: {}", e))?;
            Ok(true)
        })
        .await
    }

    /// 创建表（如果不存在）。
    ///
    /// 达梦兼容标准 SQL DDL，使用双引号转义表名和列名。
    /// 先查询 `user_tables` 判断表是否存在，避免兼容性问题。
    async fn create_table(&self, metadata: &FlgMetadata) -> Result<()> {
        let table_name = metadata.table_name.clone();
        let columns: Vec<(String, String)> = metadata
            .columns
            .iter()
            .map(|col| (col.name.clone(), column_type_to_dm_ddl(col)))
            .collect();

        self.with_connection(move |conn| {
            // 查询表是否已存在
            let check_sql = format!(
                "SELECT COUNT(*) FROM user_tables WHERE table_name = UPPER('{}')",
                table_name
            );
            let exists = if let Some(mut cursor) = conn
                .execute(&check_sql, ())
                .map_err(|e| anyhow::anyhow!("查询表是否存在失败: {}", e))?
            {
                let mut count_str = Vec::new();
                if let Some(mut row) = cursor
                    .next_row()
                    .map_err(|e| anyhow::anyhow!("读取查询结果失败: {}", e))?
                {
                    row.get_text(1, &mut count_str)
                        .map_err(|e| anyhow::anyhow!("获取计数值失败: {}", e))?;
                    let count: i64 = String::from_utf8_lossy(&count_str)
                        .trim()
                        .parse()
                        .unwrap_or(0);
                    count > 0
                } else {
                    false
                }
            } else {
                false
            };

            if exists {
                info!("表 \"{}\" 已存在，跳过创建", table_name);
                return Ok(());
            }

            // 构建 CREATE TABLE DDL
            let columns_ddl: Vec<String> = columns
                .iter()
                .map(|(name, type_str)| format!("  \"{}\" {}", name, type_str))
                .collect();

            let sql = format!(
                "CREATE TABLE \"{}\" (\n{}\n)",
                table_name,
                columns_ddl.join(",\n")
            );

            conn.execute(&sql, ())
                .map_err(|e| anyhow::anyhow!("创建表 \"{}\" 失败: {}", table_name, e))?;
            info!("表 \"{}\" 创建完成", table_name);
            Ok(())
        })
        .await
    }

    /// 批量插入数据。
    ///
    /// 使用 prepared statement 逐行绑定参数执行插入。
    /// 单行失败记录警告但不中断后续批次。
    async fn batch_insert(
        &self,
        table_name: &str,
        metadata: &FlgMetadata,
        rows: &[DataRow],
    ) -> Result<usize> {
        if rows.is_empty() {
            return Ok(0);
        }

        let col_count = metadata.columns.len();
        let batch_size = safe_batch_size(col_count, rows.len());
        let table_name = table_name.to_string();
        let col_names: Vec<String> = metadata
            .columns
            .iter()
            .map(|c| format!("\"{}\"", c.name))
            .collect();
        let rows_owned: Vec<DataRow> = rows.to_vec();

        self.with_connection(move |conn| {
            // 关闭自动提交，启用手动事务
            conn.set_autocommit(false)
                .map_err(|e| anyhow::anyhow!("设置手动提交模式失败: {}", e))?;

            let placeholders = vec!["?"; col_count].join(",");
            let sql = format!(
                "INSERT INTO \"{}\" ({}) VALUES ({})",
                table_name,
                col_names.join(","),
                placeholders
            );

            let mut total_affected: usize = 0;

            for chunk in rows_owned.chunks(batch_size) {
                let mut prepared = conn
                    .prepare(&sql)
                    .map_err(|e| anyhow::anyhow!("准备插入语句失败: {}", e))?;

                for row in chunk {
                    let params: Vec<_> = row
                        .values
                        .iter()
                        .map(|v| v.as_str().into_parameter())
                        .collect();

                    match prepared.execute(params.as_slice()) {
                        Ok(_) => {
                            total_affected += 1;
                        }
                        Err(e) => {
                            warn!("插入行失败（表 {}）: {}", table_name, e);
                        }
                    }
                }
            }

            // 提交事务
            conn.commit()
                .map_err(|e| anyhow::anyhow!("提交事务失败: {}", e))?;

            Ok(total_affected)
        })
        .await
    }

    /// 获取表中已有的行数。
    async fn get_row_count(&self, table_name: &str) -> Result<usize> {
        let table_name = table_name.to_string();
        self.with_connection(move |conn| {
            let sql = format!("SELECT COUNT(*) FROM \"{}\"", table_name);
            let mut cursor = conn
                .execute(&sql, ())
                .map_err(|e| anyhow::anyhow!("查询行数失败: {}", e))?
                .ok_or_else(|| anyhow::anyhow!("COUNT 查询未返回结果集"))?;

            let mut buf = Vec::new();
            if let Some(mut row) = cursor
                .next_row()
                .map_err(|e| anyhow::anyhow!("读取行数结果失败: {}", e))?
            {
                row.get_text(1, &mut buf)
                    .map_err(|e| anyhow::anyhow!("获取计数值失败: {}", e))?;
                let count: usize = String::from_utf8_lossy(&buf).trim().parse().unwrap_or(0);
                Ok(count)
            } else {
                Ok(0)
            }
        })
        .await
    }

    /// 关闭连接。
    ///
    /// 由于每次操作都在 spawn_blocking 内独立创建连接，无需额外清理。
    async fn close(&self) -> Result<()> {
        info!("达梦数据库连接已关闭（无持久连接需释放）");
        Ok(())
    }
}
