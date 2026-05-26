//! MySQL 数据库适配实现。
//!
//! 同时支持 MySQL 协议兼容的数据库：TXSQL、TDSQL。

use super::DatabaseLoader;
use crate::models::{DataRow, DatabaseConfig, FlgMetadata};
use anyhow::{Context, Result};
use async_trait::async_trait;
use sqlx::mysql::{MySqlConnectOptions, MySqlPool, MySqlPoolOptions};
use sqlx::Row;
use tracing::{debug, info};

/// MySQL 加载器
pub struct MysqlLoader {
    pool: MySqlPool,
}

impl MysqlLoader {
    /// 根据配置创建 MySQL 连接池。
    pub async fn new(config: &DatabaseConfig) -> Result<Self> {
        let options = MySqlConnectOptions::new()
            .host(&config.host)
            .port(config.port)
            .username(&config.username)
            .password(&config.password)
            .database(&config.database);

        info!(
            "连接 MySQL: {}:{}/{}",
            config.host, config.port, config.database
        );
        let pool = MySqlPoolOptions::new()
            .max_connections(10)
            .min_connections(1)
            .acquire_timeout(std::time::Duration::from_secs(30))
            .connect_with(options)
            .await
            .with_context(|| {
                format!(
                    "连接 MySQL 失败: {}:{}/{}",
                    config.host, config.port, config.database
                )
            })?;
        Ok(Self { pool })
    }
}

#[async_trait]
impl DatabaseLoader for MysqlLoader {
    async fn test_connection(&self) -> Result<bool> {
        sqlx::query("SELECT 1").execute(&self.pool).await?;
        Ok(true)
    }

    async fn create_table(&self, metadata: &FlgMetadata) -> Result<()> {
        let columns_ddl: Vec<String> = metadata
            .columns
            .iter()
            .map(|col| format!("  `{}` {}", col.name, super::column_type_to_mysql_ddl(col)))
            .collect();

        let sql = format!(
            "CREATE TABLE IF NOT EXISTS `{}` (\n{}\n)",
            metadata.table_name,
            columns_ddl.join(",\n")
        );

        debug!("MySQL CREATE TABLE:\n{}", sql);
        sqlx::query(&sql)
            .execute(&self.pool)
            .await
            .with_context(|| format!("创建表 {} 失败", metadata.table_name))?;
        info!("表 {} 创建完成（或已存在）", metadata.table_name);
        Ok(())
    }

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
        let batch_size = super::safe_batch_size(col_count, rows.len());
        let mut total_affected: usize = 0;

        for chunk in rows.chunks(batch_size) {
            let col_names: Vec<String> = metadata
                .columns
                .iter()
                .map(|c| format!("`{}`", c.name))
                .collect();
            let placeholders_row = format!("({})", vec!["?"; col_count].join(","));
            let all_placeholders: Vec<&str> = vec![placeholders_row.as_str(); chunk.len()];

            let sql = format!(
                "INSERT INTO `{}` ({}) VALUES {}",
                table_name,
                col_names.join(","),
                all_placeholders.join(",")
            );

            let mut query = sqlx::query(&sql);
            for row in chunk {
                for value in &row.values {
                    query = query.bind(value);
                }
            }

            let result = query
                .execute(&self.pool)
                .await
                .with_context(|| format!("批量插入表 {} 失败", table_name))?;
            total_affected += result.rows_affected() as usize;
        }

        Ok(total_affected)
    }

    async fn get_row_count(&self, table_name: &str) -> Result<usize> {
        let sql = format!("SELECT COUNT(*) as cnt FROM `{}`", table_name);
        let row = sqlx::query(&sql)
            .fetch_one(&self.pool)
            .await
            .with_context(|| format!("查询表 {} 行数失败", table_name))?;
        let count: i64 = row.get("cnt");
        Ok(count as usize)
    }

    async fn close(&self) -> Result<()> {
        self.pool.close().await;
        info!("MySQL 连接池已关闭");
        Ok(())
    }
}
