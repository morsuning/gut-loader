//! PostgreSQL/openGauss/GaussDB 等 PG 协议兼容数据库适配实现。

use super::DatabaseLoader;
use crate::models::{ColumnType, DataRow, DatabaseConfig, FlgMetadata};
use anyhow::{Context, Result};
use async_trait::async_trait;
use sqlx::postgres::{PgConnectOptions, PgPool, PgPoolOptions};
use sqlx::Row;
use tracing::{debug, info};

/// 将列类型转换为 PostgreSQL 占位符类型转换后缀（例如 ::BIGINT、::NUMERIC、::VARCHAR）。
/// 用于安全地以文本形式绑定参数后让 PostgreSQL 自动转换为目标列类型。
fn pg_cast_suffix(col_type: &ColumnType) -> &'static str {
    match col_type {
        ColumnType::Varchar(_) => "::TEXT",
        ColumnType::Decimal(_, _) => "::NUMERIC",
        ColumnType::Int(_) => "::BIGINT",
    }
}

/// PostgreSQL 加载器
pub struct PostgresLoader {
    pool: PgPool,
    schema: Option<String>,
}

impl PostgresLoader {
    /// 根据配置创建 PostgreSQL 连接池。
    pub async fn new(config: &DatabaseConfig) -> Result<Self> {
        let mut options = PgConnectOptions::new()
            .host(&config.host)
            .port(config.port)
            .username(&config.username)
            .password(&config.password)
            .database(&config.database);

        // 如果指定了 schema，则设置 search_path
        if let Some(schema) = &config.schema {
            options = options.options([("search_path", schema.as_str())]);
        }

        info!(
            "连接 PostgreSQL: {}:{}/{}",
            config.host, config.port, config.database
        );
        let pool = PgPoolOptions::new()
            .max_connections(10)
            .min_connections(1)
            .acquire_timeout(std::time::Duration::from_secs(30))
            .connect_with(options)
            .await
            .with_context(|| {
                format!(
                    "连接 PostgreSQL 失败: {}:{}/{}",
                    config.host, config.port, config.database
                )
            })?;

        Ok(Self {
            pool,
            schema: config.schema.clone(),
        })
    }

    /// 获取带 schema 前缀的完整表名
    fn qualified_table_name(&self, table_name: &str) -> String {
        if let Some(schema) = &self.schema {
            format!("\"{}\".\"{}\"", schema, table_name)
        } else {
            format!("\"{}\"", table_name)
        }
    }
}

#[async_trait]
impl DatabaseLoader for PostgresLoader {
    async fn test_connection(&self) -> Result<bool> {
        sqlx::query("SELECT 1").execute(&self.pool).await?;
        Ok(true)
    }

    async fn create_table(&self, metadata: &FlgMetadata) -> Result<()> {
        let columns_ddl: Vec<String> = metadata
            .columns
            .iter()
            .map(|col| format!("  \"{}\" {}", col.name, super::column_type_to_pg_ddl(col)))
            .collect();

        let qualified_name = self.qualified_table_name(&metadata.table_name);
        let sql = format!(
            "CREATE TABLE IF NOT EXISTS {} (\n{}\n)",
            qualified_name,
            columns_ddl.join(",\n")
        );

        debug!("PostgreSQL CREATE TABLE:\n{}", sql);
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
        // PostgreSQL 参数上限约 65535，安全取 60000
        let batch_size = super::safe_batch_size(col_count, rows.len());
        let mut total_affected: usize = 0;

        let qualified_name = self.qualified_table_name(table_name);
        let col_names: Vec<String> = metadata
            .columns
            .iter()
            .map(|c| format!("\"{}\"", c.name))
            .collect();
        let col_names_str = col_names.join(",");

        // 预先计算每列的类型转换后缀，避免重复匹配
        let cast_suffixes: Vec<&'static str> = metadata
            .columns
            .iter()
            .map(|c| pg_cast_suffix(&c.data_type))
            .collect();

        for chunk in rows.chunks(batch_size) {
            // 生成 $1::TYPE, $2::TYPE, ... 占位符（带类型转换）
            let mut placeholders_parts: Vec<String> = Vec::with_capacity(chunk.len());
            let mut param_idx: usize = 1;
            for _ in chunk {
                let row_placeholders: Vec<String> = (0..col_count)
                    .map(|i| {
                        let p = format!("${}{}", param_idx, cast_suffixes[i]);
                        param_idx += 1;
                        p
                    })
                    .collect();
                placeholders_parts.push(format!("({})", row_placeholders.join(",")));
            }

            let sql = format!(
                "INSERT INTO {} ({}) VALUES {}",
                qualified_name,
                col_names_str,
                placeholders_parts.join(",")
            );

            let mut query = sqlx::query(&sql);
            for row in chunk {
                for (i, value) in row.values.iter().enumerate() {
                    // 对于数值类型，空字符串视为 NULL，避免 NUMERIC/BIGINT 转换失败
                    let is_numeric = matches!(
                        metadata.columns[i].data_type,
                        ColumnType::Decimal(_, _) | ColumnType::Int(_)
                    );
                    if is_numeric && value.trim().is_empty() {
                        query = query.bind(Option::<String>::None);
                    } else {
                        query = query.bind(value);
                    }
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
        let qualified_name = self.qualified_table_name(table_name);
        let sql = format!("SELECT COUNT(*) as cnt FROM {}", qualified_name);
        let row = sqlx::query(&sql)
            .fetch_one(&self.pool)
            .await
            .with_context(|| format!("查询表 {} 行数失败", table_name))?;
        let count: i64 = row.get("cnt");
        Ok(count as usize)
    }

    async fn close(&self) -> Result<()> {
        self.pool.close().await;
        info!("PostgreSQL 连接池已关闭");
        Ok(())
    }
}
