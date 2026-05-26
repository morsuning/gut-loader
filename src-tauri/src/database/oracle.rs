//! Oracle 数据库适配器：基于 oracle-rs 纯 Rust TNS 协议驱动。
//!
//! 通过 [`oracle_rs::Connection`] 直接以 TNS 协议建立异步会话，
//! 不依赖 Oracle Instant Client / OCI / ODPI-C 等任何系统库，
//! 因此整个二进制可以在没有 Oracle 客户端的机器上零依赖运行。

use super::{column_type_to_oracle_ddl, safe_batch_size, DatabaseLoader};
use crate::models::{ColumnType, DataRow, DatabaseConfig, FlgMetadata};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use oracle_rs::{BatchBuilder, Config, Connection, Value};
use tracing::{debug, info, warn};

/// Oracle 加载器。
///
/// 持有一个 [`oracle_rs::Connection`]。该连接内部以互斥锁串行化协议交互，
/// 因此可以在 `&self` 上并发调用 `query` / `execute` 等方法。
pub struct OracleLoader {
    conn: Connection,
    schema: Option<String>,
}

impl OracleLoader {
    /// 根据配置建立 Oracle 会话（纯 Rust TNS 协议，零系统依赖）。
    pub async fn new(config: &DatabaseConfig) -> Result<Self> {
        info!(
            "连接 Oracle: {}:{}/{} (oracle-rs 纯 Rust 驱动)",
            config.host, config.port, config.database
        );

        let oracle_config = Config::new(
            &config.host,
            config.port,
            &config.database,
            &config.username,
            &config.password,
        );

        let conn = Connection::connect_with_config(oracle_config)
            .await
            .map_err(|e| {
                anyhow!(
                    "连接 Oracle 失败 ({}:{}/{}): {}",
                    config.host,
                    config.port,
                    config.database,
                    e
                )
            })?;

        // 若指定 schema，则切换 CURRENT_SCHEMA。Oracle 标识符默认大写，
        // 这里用双引号保留原始大小写以便与建表语句匹配。
        if let Some(schema) = &config.schema {
            let alter = format!("ALTER SESSION SET CURRENT_SCHEMA = \"{}\"", schema);
            if let Err(e) = conn.execute(&alter, &[]).await {
                warn!("设置 Oracle CURRENT_SCHEMA={} 失败: {}", schema, e);
            }
        }

        Ok(Self {
            conn,
            schema: config.schema.clone(),
        })
    }

    /// 生成限定后的表名（必要时带 schema 前缀），所有标识符使用双引号转义。
    fn qualified_table_name(&self, table_name: &str) -> String {
        if let Some(schema) = &self.schema {
            format!("\"{}\".\"{}\"", schema, table_name)
        } else {
            format!("\"{}\"", table_name)
        }
    }

    /// 判定目标表是否已存在。
    ///
    /// Oracle 数据字典中存储的标识符为大写，且没有 `CREATE TABLE IF NOT EXISTS`
    /// 语法，需要先查询数据字典：指定 schema 时查 `all_tables`，否则查 `user_tables`。
    async fn table_exists(&self, table_name: &str) -> Result<bool> {
        let upper_table = table_name.to_uppercase();
        let (sql, params): (&str, Vec<Value>) = if let Some(schema) = &self.schema {
            (
                "SELECT COUNT(*) FROM all_tables WHERE owner = :1 AND table_name = :2",
                vec![Value::from(schema.to_uppercase()), Value::from(upper_table)],
            )
        } else {
            (
                "SELECT COUNT(*) FROM user_tables WHERE table_name = :1",
                vec![Value::from(upper_table)],
            )
        };

        let result = self
            .conn
            .query(sql, &params)
            .await
            .map_err(|e| anyhow!("查询 Oracle 数据字典失败: {}", e))?;

        let count = result
            .rows
            .first()
            .and_then(|row| row.get_i64(0))
            .unwrap_or(0);
        Ok(count > 0)
    }
}

#[async_trait]
impl DatabaseLoader for OracleLoader {
    /// 通过 `SELECT 1 FROM DUAL` 验证连接连通性。
    async fn test_connection(&self) -> Result<bool> {
        let result = self
            .conn
            .query("SELECT 1 FROM DUAL", &[])
            .await
            .map_err(|e| anyhow!("Oracle 连接测试失败: {}", e))?;
        Ok(!result.rows.is_empty())
    }

    /// 在目标 schema 中创建表。
    ///
    /// Oracle 不支持 `IF NOT EXISTS`，因此先查询数据字典判断是否存在；
    /// 已存在则直接返回，不做任何写操作（与 PostgreSQL/MySQL 适配的语义保持一致）。
    async fn create_table(&self, metadata: &FlgMetadata) -> Result<()> {
        if self.table_exists(&metadata.table_name).await? {
            info!("表 {} 已存在，跳过创建", metadata.table_name);
            return Ok(());
        }

        let columns_ddl: Vec<String> = metadata
            .columns
            .iter()
            .map(|col| format!("  \"{}\" {}", col.name, column_type_to_oracle_ddl(col)))
            .collect();

        let qualified_name = self.qualified_table_name(&metadata.table_name);
        let sql = format!(
            "CREATE TABLE {} (\n{}\n)",
            qualified_name,
            columns_ddl.join(",\n")
        );

        debug!("Oracle CREATE TABLE:\n{}", sql);
        self.conn
            .execute(&sql, &[])
            .await
            .map_err(|e| anyhow!("创建表 {} 失败: {}", metadata.table_name, e))?;
        // DDL 在 Oracle 中本身隐式提交，这里显式 commit 以保证后续查询可见。
        self.conn
            .commit()
            .await
            .map_err(|e| anyhow!("提交建表事务失败: {}", e))?;
        info!("表 {} 创建完成", metadata.table_name);
        Ok(())
    }

    /// 批量插入数据。
    ///
    /// 实现策略：
    ///
    /// 1. 一次性构造形如 `INSERT INTO "t" ("c1", "c2") VALUES (:1, :2)` 的语句；
    ///    数值列的占位符额外用 `TO_NUMBER(NULLIF(:n, ''))` 包裹，使空串安全转为 NULL。
    /// 2. 按 [`super::safe_batch_size`] 分批，使用 [`oracle_rs::BatchBuilder`]
    ///    批量绑定参数，单次 `execute_batch` 提交。
    /// 3. 单批失败仅记录警告并继续后续批次；返回值仅累计成功行数。
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

        let qualified_name = self.qualified_table_name(table_name);
        let col_names: Vec<String> = metadata
            .columns
            .iter()
            .map(|c| format!("\"{}\"", c.name))
            .collect();

        // 数值列包装 TO_NUMBER(NULLIF(:n, ''))，VARCHAR2 列直接使用 :n 占位符；
        // 这样所有绑定参数都可以以字符串形式传入，统一了 NULL 处理逻辑。
        let placeholders: Vec<String> = metadata
            .columns
            .iter()
            .enumerate()
            .map(|(i, col)| {
                let idx = i + 1;
                match col.data_type {
                    ColumnType::Decimal(_, _) | ColumnType::Int(_) => {
                        format!("TO_NUMBER(NULLIF(:{}, ''))", idx)
                    }
                    ColumnType::Varchar(_) => format!(":{}", idx),
                }
            })
            .collect();

        let sql = format!(
            "INSERT INTO {} ({}) VALUES ({})",
            qualified_name,
            col_names.join(","),
            placeholders.join(",")
        );
        debug!("Oracle INSERT SQL: {}", sql);

        let mut total_affected: usize = 0;
        for chunk in rows.chunks(batch_size) {
            let mut builder = BatchBuilder::new(sql.clone());
            let mut chunk_count: usize = 0;
            for (row_idx, row) in chunk.iter().enumerate() {
                if row.values.len() != col_count {
                    warn!(
                        "表 {} 第 {} 行字段数不匹配（期望 {}，实际 {}），已跳过",
                        table_name,
                        row_idx,
                        col_count,
                        row.values.len()
                    );
                    continue;
                }
                let bind_values: Vec<Value> = row
                    .values
                    .iter()
                    .map(|s| Value::from(s.clone()))
                    .collect();
                builder = builder.add_row(bind_values);
                chunk_count += 1;
            }
            if chunk_count == 0 {
                continue;
            }

            let batch = builder.build();
            match self.conn.execute_batch(&batch).await {
                Ok(result) => {
                    total_affected += result.total_rows_affected as usize;
                }
                Err(e) => {
                    warn!("表 {} 批次插入失败: {}", table_name, e);
                }
            }
            if let Err(e) = self.conn.commit().await {
                warn!("表 {} 批次提交失败: {}", table_name, e);
            }
        }

        Ok(total_affected)
    }

    /// 查询表行数，用于断点续传与最终对账。
    async fn get_row_count(&self, table_name: &str) -> Result<usize> {
        let qualified_name = self.qualified_table_name(table_name);
        let sql = format!("SELECT COUNT(*) FROM {}", qualified_name);
        let result = self
            .conn
            .query(&sql, &[])
            .await
            .map_err(|e| anyhow!("查询表 {} 行数失败: {}", table_name, e))?;
        let count = result
            .rows
            .first()
            .and_then(|r| r.get_i64(0))
            .unwrap_or(0);
        Ok(count.max(0) as usize)
    }

    /// 关闭会话。
    ///
    /// 调用 [`oracle_rs::Connection::close`] 主动发送 logoff 报文并断开 TCP 连接；
    /// 如果连接已关闭则视为成功，与其它适配器保持接口一致。
    async fn close(&self) -> Result<()> {
        if self.conn.is_closed() {
            info!("Oracle 连接已处于关闭状态");
            return Ok(());
        }
        match self.conn.close().await {
            Ok(_) => {
                info!("Oracle 连接已关闭");
                Ok(())
            }
            Err(e) => {
                warn!("关闭 Oracle 连接失败: {}", e);
                Ok(())
            }
        }
    }
}
