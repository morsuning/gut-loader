//! 数据库适配模块：统一抽象多种关系型数据库的连接与批量入库能力。

pub mod dm;
pub mod mysql;
pub mod oracle;
pub mod postgres;

use crate::models::{ColumnDefinition, ColumnType, DataRow, DatabaseConfig, FlgMetadata};
use anyhow::Result;
use async_trait::async_trait;

/// 数据库加载器统一接口
#[async_trait]
pub trait DatabaseLoader: Send + Sync {
    /// 测试连接是否成功
    async fn test_connection(&self) -> Result<bool>;

    /// 创建表（如果不存在）
    async fn create_table(&self, metadata: &FlgMetadata) -> Result<()>;

    /// 批量插入数据
    async fn batch_insert(
        &self,
        table_name: &str,
        metadata: &FlgMetadata,
        rows: &[DataRow],
    ) -> Result<usize>;

    /// 获取已插入的行数（用于断点续传）
    async fn get_row_count(&self, table_name: &str) -> Result<usize>;

    /// 关闭连接
    async fn close(&self) -> Result<()>;
}

/// 根据配置创建对应的数据库加载器
pub async fn create_loader(config: &DatabaseConfig) -> Result<Box<dyn DatabaseLoader>> {
    match config.db_type.to_lowercase().as_str() {
        "mysql" | "txsql" | "tdsql" => {
            let loader = mysql::MysqlLoader::new(config).await?;
            Ok(Box::new(loader))
        }
        "postgresql" | "postgres" | "opengauss" | "gaussdb" => {
            let loader = postgres::PostgresLoader::new(config).await?;
            Ok(Box::new(loader))
        }
        "oracle" => {
            let loader = oracle::OracleLoader::new(config).await?;
            Ok(Box::new(loader))
        }
        "dameng" | "dm" => {
            let loader = dm::DmLoader::new(config).await?;
            Ok(Box::new(loader))
        }
        _ => anyhow::bail!("Unsupported database type: {}", config.db_type),
    }
}

/// 根据 ColumnType 生成 MySQL DDL 类型字符串
pub fn column_type_to_mysql_ddl(col: &ColumnDefinition) -> String {
    match &col.data_type {
        ColumnType::Varchar(n) => format!("VARCHAR({})", n),
        ColumnType::Decimal(m, n) => format!("DECIMAL({},{})", m, n),
        ColumnType::Int(_) => "BIGINT".to_string(),
    }
}

/// 根据 ColumnType 生成 PostgreSQL DDL 类型字符串
pub fn column_type_to_pg_ddl(col: &ColumnDefinition) -> String {
    match &col.data_type {
        ColumnType::Varchar(n) => format!("VARCHAR({})", n),
        ColumnType::Decimal(m, n) => format!("NUMERIC({},{})", m, n),
        ColumnType::Int(_) => "BIGINT".to_string(),
    }
}

/// 根据 ColumnType 生成 Oracle DDL 类型字符串
pub fn column_type_to_oracle_ddl(col: &ColumnDefinition) -> String {
    match &col.data_type {
        ColumnType::Varchar(n) => format!("VARCHAR2({})", n),
        ColumnType::Decimal(m, n) => format!("NUMBER({},{})", m, n),
        ColumnType::Int(_) => "NUMBER(19)".to_string(),
    }
}

/// 根据 ColumnType 生成达梦 DM DDL 类型字符串
pub fn column_type_to_dm_ddl(col: &ColumnDefinition) -> String {
    match &col.data_type {
        ColumnType::Varchar(n) => format!("VARCHAR({})", n),
        ColumnType::Decimal(m, n) => format!("DECIMAL({},{})", m, n),
        ColumnType::Int(_) => "BIGINT".to_string(),
    }
}

/// 计算安全的批次大小，确保不超过数据库参数限制。
///
/// PostgreSQL 最多约 65535 个参数，MySQL 通常也有类似限制。
pub fn safe_batch_size(column_count: usize, desired_batch_size: usize) -> usize {
    const MAX_PARAMS: usize = 60000;
    if column_count == 0 {
        return desired_batch_size;
    }
    let max_rows = MAX_PARAMS / column_count;
    desired_batch_size.min(max_rows).max(1)
}
