//! 通用数据模型定义：跨模块共享的结构体与类型。

use serde::{Deserialize, Serialize};

/// 字段数据类型。
///
/// 对应 .flg 文件中的字段类型描述：
/// - `VARCHAR(n)`：变长字符串，n 表示最大字节长度
/// - `DECIMAL(m,n)`：定点数，m 为总位数，n 为小数位数
/// - `INT(n)`：整数，n 表示存储位宽
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ColumnType {
    /// VARCHAR(n)
    Varchar(usize),
    /// DECIMAL(m,n)
    Decimal(usize, usize),
    /// INT(n)
    Int(usize),
}

/// 字段定义。
///
/// 对应 .flg 文件中 `序号$$字段名$$数据类型$$（起始位置,结束位置）` 的一行描述。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnDefinition {
    /// 序号（1-based）
    pub index: usize,
    /// 字段名
    pub name: String,
    /// 数据类型
    pub data_type: ColumnType,
    /// 起始字节位置（1-based）
    pub start_pos: usize,
    /// 结束字节位置（1-based，闭区间）
    pub end_pos: usize,
}

/// FLG 元数据。
///
/// 描述一个 .flg 文件中的全部信息，是后续 .dat.gz 文件解析的依据。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlgMetadata {
    /// 压缩文件名（FILENAME）
    pub filename: String,
    /// 文件大小（KB，FILESIZE）
    pub file_size: u64,
    /// 行数（ROWCOUNT）
    pub row_count: usize,
    /// 创建时间（CREATEDATETIME）
    pub created_at: String,
    /// 原始 SQL（SQL）
    pub sql: String,
    /// 单行字节长度（ROWLENGTH）
    pub row_length: usize,
    /// 字段数（COLUMNCOUNT）
    pub column_count: usize,
    /// 字段定义列表
    pub columns: Vec<ColumnDefinition>,
    /// 表名（从文件名解析）
    pub table_name: String,
}

/// 解析后的数据行。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataRow {
    /// 各字段值（已 trim 右侧空格）
    pub values: Vec<String>,
}

/// GUT 文件对（flg + dat.gz）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GutFilePair {
    pub table_name: String,
    pub date: String,
    pub time: String,
    pub sequence: String,
    pub flg_path: String,
    pub dat_path: String,
}

/// 数据库连接配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    /// 数据库类型：mysql, postgresql, opengauss, txsql, tdsql, gaussdb, oracle, dameng
    pub db_type: String,
    pub host: String,
    pub port: u16,
    pub database: String,
    pub username: String,
    pub password: String,
    pub schema: Option<String>,
}

/// LLM 配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    pub api_url: String,
    pub api_key: String,
    pub model: String,
}

/// 加载进度。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadProgress {
    pub table_name: String,
    pub total_rows: usize,
    pub loaded_rows: usize,
    pub failed_rows: usize,
    /// 状态：pending, loading, completed, failed
    pub status: String,
    /// 速度（行/秒）
    pub speed: f64,
    pub elapsed_ms: u64,
}

/// 加载报告。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadReport {
    pub total_tables: usize,
    pub total_rows: usize,
    pub success_rows: usize,
    pub failed_rows: usize,
    pub success_rate: f64,
    pub total_elapsed_ms: u64,
    /// 平均速度（行/秒）
    pub avg_speed: f64,
    pub table_reports: Vec<TableReport>,
}

/// 单表加载报告。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableReport {
    pub table_name: String,
    pub row_count: usize,
    pub success_count: usize,
    pub failed_count: usize,
    pub elapsed_ms: u64,
    pub speed: f64,
    pub errors: Vec<String>,
}

/// 前置检查结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreCheckResult {
    pub check_name: String,
    pub passed: bool,
    pub message: String,
    /// 严重程度：info, warning, error
    pub severity: String,
}
