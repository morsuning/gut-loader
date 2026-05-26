//! flg 文件解析：包含表结构、字段格式等元数据信息。

use std::fs;
use std::path::Path;

use anyhow::{anyhow, Context, Result};
use tracing::{debug, warn};

use crate::models::{ColumnDefinition, ColumnType, FlgMetadata};

/// 解析 .flg 文件，返回 [`FlgMetadata`]。
///
/// FLG 文件格式：
/// 1. 第一行为快速摘要（`原始DAT文件名 原始大小 行数 创建时间`），解析时跳过；
/// 2. 之后是若干键值对（`KEY=VALUE`）；
/// 3. `COLUMNDECRIPTION=` 之后每一行是一条字段定义，使用 `$$` 分隔。
pub fn parse_flg(path: &Path) -> Result<FlgMetadata> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("读取 flg 文件失败: {}", path.display()))?;

    let table_name = extract_table_name(path)
        .with_context(|| format!("无法从文件名解析表名: {}", path.display()))?;

    let mut filename = String::new();
    let mut file_size: u64 = 0;
    let mut row_count: usize = 0;
    let mut created_at = String::new();
    let mut sql = String::new();
    let mut row_length: usize = 0;
    let mut column_count: usize = 0;
    let mut columns: Vec<ColumnDefinition> = Vec::new();

    let mut in_columns = false;

    // 跳过第一行（摘要行）
    let mut lines = content.lines();
    let _summary = lines.next();

    for raw_line in lines {
        let line = raw_line.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            continue;
        }

        if in_columns {
            // 字段定义行
            match parse_column_line(line) {
                Ok(col) => columns.push(col),
                Err(err) => {
                    warn!("忽略无法解析的字段定义行 `{}`: {}", line, err);
                }
            }
            continue;
        }

        // 键值对
        let Some((key, value)) = line.split_once('=') else {
            warn!("忽略无法解析的行: `{}`", line);
            continue;
        };
        let key = key.trim();
        let value = value.trim();

        match key {
            "FILENAME" => filename = value.to_string(),
            "FILESIZE" => {
                file_size = value
                    .parse::<u64>()
                    .with_context(|| format!("FILESIZE 不是合法整数: `{}`", value))?;
            }
            "ROWCOUNT" => {
                row_count = value
                    .parse::<usize>()
                    .with_context(|| format!("ROWCOUNT 不是合法整数: `{}`", value))?;
            }
            "CREATEDATETIME" => created_at = value.to_string(),
            "SQL" => sql = value.to_string(),
            "ROWLENGTH" => {
                row_length = value
                    .parse::<usize>()
                    .with_context(|| format!("ROWLENGTH 不是合法整数: `{}`", value))?;
            }
            "COLUMNCOUNT" => {
                column_count = value
                    .parse::<usize>()
                    .with_context(|| format!("COLUMNCOUNT 不是合法整数: `{}`", value))?;
            }
            "COLUMNDECRIPTION" => {
                // 自此之后的每一行均为字段定义。
                in_columns = true;
            }
            other => {
                debug!("未识别的键: {}={}", other, value);
            }
        }
    }

    if column_count != 0 && columns.len() != column_count {
        warn!(
            "字段定义数量({}) 与 COLUMNCOUNT({}) 不一致",
            columns.len(),
            column_count
        );
    }

    Ok(FlgMetadata {
        filename,
        file_size,
        row_count,
        created_at,
        sql,
        row_length,
        column_count,
        columns,
        table_name,
    })
}

/// 从 flg 文件路径中提取表名。
///
/// 命名规则 `<TABLE_NAME>.<YYYYMMDD>.<HHMMSS>.<SEQUENCE>.<EXTENSION>`，
/// 取第一个 `.` 之前的部分作为表名。
fn extract_table_name(path: &Path) -> Result<String> {
    let stem = path
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| anyhow!("文件名为空或非 UTF-8"))?;

    let table = stem
        .split('.')
        .next()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow!("文件名格式非法: {}", stem))?;

    Ok(table.to_string())
}

/// 解析单条字段定义行：`序号$$字段名$$数据类型$$（起始位置,结束位置）`。
fn parse_column_line(line: &str) -> Result<ColumnDefinition> {
    let parts: Vec<&str> = line.split("$$").collect();
    if parts.len() != 4 {
        return Err(anyhow!(
            "字段定义应包含 4 段（$$ 分隔），实际 {} 段: `{}`",
            parts.len(),
            line
        ));
    }

    let index: usize = parts[0]
        .trim()
        .parse()
        .with_context(|| format!("序号不是合法整数: `{}`", parts[0]))?;
    let name = parts[1].trim().to_string();
    let data_type = parse_column_type(parts[2].trim())?;
    let (start_pos, end_pos) = parse_position_range(parts[3].trim())?;

    if start_pos == 0 || end_pos < start_pos {
        return Err(anyhow!("字段位置区间非法: ({}, {})", start_pos, end_pos));
    }

    Ok(ColumnDefinition {
        index,
        name,
        data_type,
        start_pos,
        end_pos,
    })
}

/// 解析数据类型，例如 `VARCHAR(50)`、`DECIMAL(12,2)`、`INT(10)`。
fn parse_column_type(s: &str) -> Result<ColumnType> {
    let s = s.trim();
    let lparen = s
        .find('(')
        .ok_or_else(|| anyhow!("数据类型缺少左括号: `{}`", s))?;
    let rparen = s
        .rfind(')')
        .ok_or_else(|| anyhow!("数据类型缺少右括号: `{}`", s))?;
    if rparen <= lparen {
        return Err(anyhow!("数据类型括号位置异常: `{}`", s));
    }
    let kind = s[..lparen].trim().to_ascii_uppercase();
    let inner = &s[lparen + 1..rparen];

    match kind.as_str() {
        "VARCHAR" => {
            let n: usize = inner
                .trim()
                .parse()
                .with_context(|| format!("VARCHAR 长度非法: `{}`", inner))?;
            Ok(ColumnType::Varchar(n))
        }
        "DECIMAL" | "NUMERIC" => {
            let mut iter = inner.split(',');
            let m_str = iter
                .next()
                .ok_or_else(|| anyhow!("DECIMAL 缺少总位数: `{}`", inner))?;
            let n_str = iter
                .next()
                .ok_or_else(|| anyhow!("DECIMAL 缺少小数位数: `{}`", inner))?;
            let m: usize = m_str
                .trim()
                .parse()
                .with_context(|| format!("DECIMAL 总位数非法: `{}`", m_str))?;
            let n: usize = n_str
                .trim()
                .parse()
                .with_context(|| format!("DECIMAL 小数位数非法: `{}`", n_str))?;
            Ok(ColumnType::Decimal(m, n))
        }
        "INT" | "INTEGER" => {
            let n: usize = inner
                .trim()
                .parse()
                .with_context(|| format!("INT 位宽非法: `{}`", inner))?;
            Ok(ColumnType::Int(n))
        }
        other => Err(anyhow!("不支持的数据类型: `{}`", other)),
    }
}

/// 解析位置区间，支持半角 `(s,e)` 与全角 `（s,e）`。
fn parse_position_range(s: &str) -> Result<(usize, usize)> {
    let trimmed = s.trim();
    // 同时去除半角与全角括号
    let inner = trimmed
        .trim_start_matches(['(', '（'])
        .trim_end_matches([')', '）']);

    let mut iter = inner.split([',', '，']);
    let start = iter
        .next()
        .ok_or_else(|| anyhow!("位置区间缺少起始位置: `{}`", s))?
        .trim();
    let end = iter
        .next()
        .ok_or_else(|| anyhow!("位置区间缺少结束位置: `{}`", s))?
        .trim();

    let start_pos: usize = start
        .parse()
        .with_context(|| format!("起始位置非法: `{}`", start))?;
    let end_pos: usize = end
        .parse()
        .with_context(|| format!("结束位置非法: `{}`", end))?;
    Ok((start_pos, end_pos))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn example_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("example_data")
    }

    #[test]
    fn test_parse_flg_employee() {
        let path = example_dir().join("employee.20260421.000000.0000.flg");
        let meta = parse_flg(&path).expect("解析 employee.flg 失败");
        assert_eq!(meta.table_name, "employee");
        assert_eq!(meta.filename, "employee.20260421.000000.0000.dat.gz");
        assert_eq!(meta.row_count, 800);
        assert_eq!(meta.row_length, 212);
        assert_eq!(meta.column_count, 6);
        assert_eq!(meta.columns.len(), 6);
        assert_eq!(meta.columns[0].name, "EMP_NO");
        assert_eq!(meta.columns[0].start_pos, 1);
        assert_eq!(meta.columns[0].end_pos, 20);
        assert!(matches!(meta.columns[0].data_type, ColumnType::Varchar(20)));
        assert!(matches!(
            meta.columns[4].data_type,
            ColumnType::Decimal(12, 2)
        ));
    }

    #[test]
    fn test_parse_flg_order_int() {
        let path = example_dir().join("order.20260421.000000.0000.flg");
        let meta = parse_flg(&path).expect("解析 order.flg 失败");
        assert_eq!(meta.table_name, "order");
        assert_eq!(meta.row_count, 3000);
        assert_eq!(meta.row_length, 205);
        let qty = meta.columns.iter().find(|c| c.name == "QUANTITY").unwrap();
        assert!(matches!(qty.data_type, ColumnType::Int(10)));
    }

    #[test]
    fn test_parse_flg_user() {
        let path = example_dir().join("user.20260421.000000.0000.flg");
        let meta = parse_flg(&path).expect("解析 user.flg 失败");
        assert_eq!(meta.table_name, "user");
        assert_eq!(meta.column_count, 5);
        assert_eq!(meta.columns.len(), 5);
        assert_eq!(meta.row_length, 195);
    }

    #[test]
    fn test_parse_column_type() {
        assert!(matches!(
            parse_column_type("VARCHAR(50)").unwrap(),
            ColumnType::Varchar(50)
        ));
        assert!(matches!(
            parse_column_type("DECIMAL(12,2)").unwrap(),
            ColumnType::Decimal(12, 2)
        ));
        assert!(matches!(
            parse_column_type("INT(10)").unwrap(),
            ColumnType::Int(10)
        ));
    }

    #[test]
    fn test_parse_position_range_halfwidth() {
        assert_eq!(parse_position_range("(1,20)").unwrap(), (1, 20));
    }

    #[test]
    fn test_parse_position_range_fullwidth() {
        assert_eq!(parse_position_range("（21,70）").unwrap(), (21, 70));
    }
}
