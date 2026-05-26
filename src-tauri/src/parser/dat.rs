//! dat 文件解析：定长数据文件（通常为 .dat.gz 压缩格式）。

use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;

use anyhow::{anyhow, Context, Result};
use flate2::read::GzDecoder;
use tracing::{debug, warn};

use crate::models::{ColumnDefinition, DataRow, FlgMetadata};

/// 解析整份 .dat / .dat.gz 文件，返回所有数据行。
///
/// 行终止符为 CRLF（`\r\n`），不计入 `ROWLENGTH`。
/// 字段提取基于字节偏移（1-based，闭区间），可正确处理 UTF-8 多字节字符。
/// 异常行会记录日志但不会中断解析。
pub fn parse_dat(dat_path: &Path, metadata: &FlgMetadata) -> Result<Vec<DataRow>> {
    let mut rows: Vec<DataRow> = Vec::with_capacity(metadata.row_count);
    let mut errors: usize = 0;

    parse_dat_with_callback(dat_path, metadata, |line_no, result| {
        match result {
            Ok(row) => rows.push(row),
            Err(err) => {
                errors += 1;
                warn!("第 {} 行解析失败: {}", line_no, err);
            }
        }
        Ok(())
    })?;

    if rows.len() != metadata.row_count {
        warn!(
            "已解析行数({}) 与 ROWCOUNT({}) 不一致，错误行数 {}",
            rows.len(),
            metadata.row_count,
            errors
        );
    }

    Ok(rows)
}

/// 流式解析 .dat / .dat.gz 文件，每凑齐 `batch_size` 行调用一次 `on_batch`。
///
/// 适合大文件分批写入数据库的场景。
pub fn parse_dat_streaming<F>(
    dat_path: &Path,
    metadata: &FlgMetadata,
    batch_size: usize,
    mut on_batch: F,
) -> Result<()>
where
    F: FnMut(Vec<DataRow>) -> Result<()>,
{
    if batch_size == 0 {
        return Err(anyhow!("batch_size 不能为 0"));
    }

    let mut buffer: Vec<DataRow> = Vec::with_capacity(batch_size);

    parse_dat_with_callback(dat_path, metadata, |line_no, result| {
        match result {
            Ok(row) => {
                buffer.push(row);
                if buffer.len() >= batch_size {
                    let batch = std::mem::replace(&mut buffer, Vec::with_capacity(batch_size));
                    on_batch(batch)?;
                }
            }
            Err(err) => {
                warn!("第 {} 行解析失败: {}", line_no, err);
            }
        }
        Ok(())
    })?;

    if !buffer.is_empty() {
        on_batch(buffer)?;
    }

    Ok(())
}

/// 使用回调方式逐行解析 .dat / .dat.gz 文件。
///
/// 回调参数：
/// - `line_no`：1-based 行号
/// - `result`：解析结果，错误时由调用方决定是否中断
fn parse_dat_with_callback<F>(dat_path: &Path, metadata: &FlgMetadata, mut on_row: F) -> Result<()>
where
    F: FnMut(usize, Result<DataRow>) -> Result<()>,
{
    let file = File::open(dat_path)
        .with_context(|| format!("打开 dat 文件失败: {}", dat_path.display()))?;

    let reader: Box<dyn Read> = if is_gzip(dat_path) {
        Box::new(GzDecoder::new(file))
    } else {
        Box::new(file)
    };

    let mut buf_reader = BufReader::with_capacity(64 * 1024, reader);

    let mut line_no: usize = 0;
    let mut line_buf: Vec<u8> = Vec::with_capacity(metadata.row_length + 2);
    loop {
        line_buf.clear();
        let n = buf_reader
            .read_until(b'\n', &mut line_buf)
            .with_context(|| format!("读取 dat 文件出错: {}", dat_path.display()))?;
        if n == 0 {
            break;
        }
        // 去除尾部 \n 与 \r
        if line_buf.last() == Some(&b'\n') {
            line_buf.pop();
        }
        if line_buf.last() == Some(&b'\r') {
            line_buf.pop();
        }
        if line_buf.is_empty() {
            continue;
        }

        line_no += 1;
        let result = parse_row_bytes(&line_buf, metadata, line_no);
        on_row(line_no, result)?;
    }

    debug!(
        "dat 解析完成: {} 行（期望 {}）",
        line_no, metadata.row_count
    );
    Ok(())
}

/// 根据扩展名判断是否为 gzip。
pub(crate) fn is_gzip(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("gz"))
        .unwrap_or(false)
}

/// 按字段定义切分一行字节，返回 [`DataRow`]。
pub(crate) fn parse_row_bytes(
    bytes: &[u8],
    metadata: &FlgMetadata,
    line_no: usize,
) -> Result<DataRow> {
    if bytes.len() != metadata.row_length {
        return Err(anyhow!(
            "行长度不匹配（第 {} 行）：实际 {} 字节，期望 {} 字节",
            line_no,
            bytes.len(),
            metadata.row_length
        ));
    }

    let mut values: Vec<String> = Vec::with_capacity(metadata.columns.len());
    for col in &metadata.columns {
        values.push(extract_field(bytes, col)?);
    }
    Ok(DataRow { values })
}

/// 从字节切片中按 1-based 闭区间提取字段，并 trim 右侧空格。
fn extract_field(bytes: &[u8], col: &ColumnDefinition) -> Result<String> {
    if col.start_pos == 0 || col.end_pos < col.start_pos {
        return Err(anyhow!(
            "字段 {} 位置区间非法: ({}, {})",
            col.name,
            col.start_pos,
            col.end_pos
        ));
    }
    let start = col.start_pos - 1;
    let end = col.end_pos; // 闭区间 -> 切片的 exclusive 上界
    if end > bytes.len() {
        return Err(anyhow!(
            "字段 {} 越界: 需要 {} 字节，实际 {} 字节",
            col.name,
            end,
            bytes.len()
        ));
    }
    let slice = &bytes[start..end];
    // 行内字段使用空格右补齐，解析后需 trim_end。
    let s =
        std::str::from_utf8(slice).with_context(|| format!("字段 {} 不是合法 UTF-8", col.name))?;
    Ok(s.trim_end_matches(' ').to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::flg::parse_flg;
    use std::path::PathBuf;

    fn example_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("example_data")
    }

    #[test]
    fn test_parse_dat_employee() {
        let dir = example_dir();
        let meta = parse_flg(&dir.join("employee.20260421.000000.0000.flg")).unwrap();
        let rows = parse_dat(&dir.join("employee.20260421.000000.0000.dat.gz"), &meta).unwrap();
        assert_eq!(rows.len(), meta.row_count);
        assert_eq!(rows[0].values.len(), meta.columns.len());
        // 第一字段为 EMP_NO，长度 20 字节
        assert!(!rows[0].values[0].is_empty());
        // 第二字段为中文姓名（UTF-8 多字节），不应为空
        assert!(!rows[0].values[1].is_empty());
        // 验证中文字段被正确提取：包含至少一个非 ASCII 字符
        let has_chinese = rows[0].values[1].chars().any(|c| !c.is_ascii());
        assert!(
            has_chinese,
            "EMP_NAME 字段应包含中文，实际为: {:?}",
            rows[0].values[1]
        );
    }

    #[test]
    fn test_parse_dat_user() {
        let dir = example_dir();
        let meta = parse_flg(&dir.join("user.20260421.000000.0000.flg")).unwrap();
        let rows = parse_dat(&dir.join("user.20260421.000000.0000.dat.gz"), &meta).unwrap();
        assert_eq!(rows.len(), 2000);
        assert_eq!(rows[0].values.len(), 5);
    }

    #[test]
    fn test_parse_dat_all_examples() {
        let dir = example_dir();
        for stem in [
            "employee.20260421.000000.0000",
            "order.20260421.000000.0000",
            "product.20260421.000000.0000",
            "transaction.20260421.000000.0000",
            "user.20260421.000000.0000",
        ] {
            let meta = parse_flg(&dir.join(format!("{}.flg", stem))).unwrap();
            let rows = parse_dat(&dir.join(format!("{}.dat.gz", stem)), &meta).unwrap();
            assert_eq!(rows.len(), meta.row_count, "{} 解析行数不匹配", stem);
            for row in &rows {
                assert_eq!(row.values.len(), meta.columns.len());
            }
        }
    }

    #[test]
    fn test_parse_dat_streaming_batches() {
        let dir = example_dir();
        let meta = parse_flg(&dir.join("user.20260421.000000.0000.flg")).unwrap();
        let mut total = 0usize;
        let mut batch_count = 0usize;
        parse_dat_streaming(
            &dir.join("user.20260421.000000.0000.dat.gz"),
            &meta,
            500,
            |batch| {
                batch_count += 1;
                total += batch.len();
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(total, meta.row_count);
        assert_eq!(batch_count, (meta.row_count + 499) / 500);
    }
}
