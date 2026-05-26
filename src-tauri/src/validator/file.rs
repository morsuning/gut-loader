//! 文件格式与一致性预校验。

use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use flate2::read::GzDecoder;
use tracing::{debug, warn};

use crate::models::{FlgMetadata, GutFilePair, PreCheckResult};
use crate::parser;
use crate::parser::flg::parse_flg;

/// 执行所有文件相关检查。
pub async fn check_all_files(directory: &Path) -> Vec<PreCheckResult> {
    let mut results = Vec::new();

    // 1. 目录存在性检查
    results.push(check_directory_exists(directory));
    if !directory.is_dir() {
        return results;
    }

    // 2. 扫描文件对
    let pairs = match parser::scan_directory(directory) {
        Ok(pairs) => pairs,
        Err(e) => {
            results.push(PreCheckResult {
                check_name: "文件扫描".to_string(),
                passed: false,
                message: format!("扫描目录失败: {}", e),
                severity: "error".to_string(),
            });
            return results;
        }
    };

    // 3. 文件对完整性检查
    results.push(check_file_pairs_found(&pairs));

    if pairs.is_empty() {
        return results;
    }

    // 4. 对每组文件进行详细检查
    for pair in &pairs {
        results.extend(check_single_file_pair(pair).await);
    }

    results
}

/// 检查目录是否存在且可读。
fn check_directory_exists(directory: &Path) -> PreCheckResult {
    let check_name = "目录存在性".to_string();

    if !directory.exists() {
        return PreCheckResult {
            check_name,
            passed: false,
            message: format!("目录不存在: {}", directory.display()),
            severity: "error".to_string(),
        };
    }

    if !directory.is_dir() {
        return PreCheckResult {
            check_name,
            passed: false,
            message: format!("路径不是目录: {}", directory.display()),
            severity: "error".to_string(),
        };
    }

    // 尝试读取目录确认可读
    match std::fs::read_dir(directory) {
        Ok(_) => PreCheckResult {
            check_name,
            passed: true,
            message: format!("目录存在且可读: {}", directory.display()),
            severity: "info".to_string(),
        },
        Err(e) => PreCheckResult {
            check_name,
            passed: false,
            message: format!("目录不可读: {}: {}", directory.display(), e),
            severity: "error".to_string(),
        },
    }
}

/// 检查是否发现了文件对。
fn check_file_pairs_found(pairs: &[GutFilePair]) -> PreCheckResult {
    let check_name = "文件扫描".to_string();

    if pairs.is_empty() {
        PreCheckResult {
            check_name,
            passed: false,
            message: "未发现任何有效的文件对（.flg + .dat.gz）".to_string(),
            severity: "error".to_string(),
        }
    } else {
        PreCheckResult {
            check_name,
            passed: true,
            message: format!("发现 {} 组有效文件对", pairs.len()),
            severity: "info".to_string(),
        }
    }
}

/// 检查单个文件对的所有验证项。
async fn check_single_file_pair(pair: &GutFilePair) -> Vec<PreCheckResult> {
    let mut results = Vec::new();

    // a. 文件命名规则校验
    results.push(check_filename_format(pair));

    // b. FLG文件可解析性
    let metadata = match check_flg_parseable(pair) {
        (result, Some(meta)) => {
            results.push(result);
            meta
        }
        (result, None) => {
            results.push(result);
            return results; // 无法继续后续检查
        }
    };

    // c. DAT.GZ文件存在且可解压
    results.push(check_dat_file_exists(pair));

    // d. 字段定义完整性（字段数 == COLUMNCOUNT）
    results.push(check_column_count(&metadata));

    // e. 字段位置范围连续性（无间隙、无重叠、不超出ROWLENGTH）
    results.push(check_column_positions(&metadata));

    // f. GZIP完整性检查（尝试解压前几行验证）
    results.push(check_gzip_integrity(pair).await);

    // g. ROWCOUNT与实际行数匹配
    results.push(check_row_count(pair, &metadata).await);

    results
}

/// 检查文件命名格式是否符合 `table.YYYYMMDD.HHMMSS.SEQUENCE.ext`。
fn check_filename_format(pair: &GutFilePair) -> PreCheckResult {
    let check_name = format!("[{}] 文件命名格式", pair.table_name);

    // 验证日期格式 YYYYMMDD
    let date_valid = pair.date.len() == 8 && pair.date.chars().all(|c| c.is_ascii_digit());

    // 验证时间格式 HHMMSS
    let time_valid = pair.time.len() == 6 && pair.time.chars().all(|c| c.is_ascii_digit());

    // 验证序号格式（纯数字）
    let seq_valid = !pair.sequence.is_empty() && pair.sequence.chars().all(|c| c.is_ascii_digit());

    // 验证表名不为空
    let name_valid = !pair.table_name.is_empty();

    let passed = date_valid && time_valid && seq_valid && name_valid;

    let message = if passed {
        format!(
            "文件命名格式正确: {}.{}.{}.{}",
            pair.table_name, pair.date, pair.time, pair.sequence
        )
    } else {
        let mut issues = Vec::new();
        if !name_valid {
            issues.push("表名为空");
        }
        if !date_valid {
            issues.push("日期格式不符合YYYYMMDD");
        }
        if !time_valid {
            issues.push("时间格式不符合HHMMSS");
        }
        if !seq_valid {
            issues.push("序号格式不正确");
        }
        format!(
            "文件命名格式异常: {}.{}.{}.{} ({})",
            pair.table_name,
            pair.date,
            pair.time,
            pair.sequence,
            issues.join(", ")
        )
    };

    PreCheckResult {
        check_name,
        passed,
        message,
        severity: "warning".to_string(),
    }
}

/// 检查 FLG 文件是否可正确解析。
fn check_flg_parseable(pair: &GutFilePair) -> (PreCheckResult, Option<FlgMetadata>) {
    let check_name = format!("[{}] FLG可解析", pair.table_name);
    let flg_path = Path::new(&pair.flg_path);

    match parse_flg(flg_path) {
        Ok(meta) => {
            let result = PreCheckResult {
                check_name,
                passed: true,
                message: format!(
                    "FLG文件解析成功: 表={}, 行数={}, 字段数={}",
                    meta.table_name, meta.row_count, meta.column_count
                ),
                severity: "info".to_string(),
            };
            (result, Some(meta))
        }
        Err(e) => {
            let result = PreCheckResult {
                check_name,
                passed: false,
                message: format!("FLG文件解析失败: {}", e),
                severity: "error".to_string(),
            };
            (result, None)
        }
    }
}

/// 检查 DAT 文件是否存在。
fn check_dat_file_exists(pair: &GutFilePair) -> PreCheckResult {
    let check_name = format!("[{}] DAT文件存在", pair.table_name);
    let dat_path = Path::new(&pair.dat_path);

    if dat_path.exists() && dat_path.is_file() {
        let size = std::fs::metadata(dat_path).map(|m| m.len()).unwrap_or(0);
        PreCheckResult {
            check_name,
            passed: true,
            message: format!("DAT文件存在, 大小: {} 字节", size),
            severity: "info".to_string(),
        }
    } else {
        PreCheckResult {
            check_name,
            passed: false,
            message: format!("DAT文件不存在: {}", dat_path.display()),
            severity: "error".to_string(),
        }
    }
}

/// 检查字段数是否与 COLUMNCOUNT 匹配。
fn check_column_count(metadata: &FlgMetadata) -> PreCheckResult {
    let check_name = format!("[{}] 字段数量", metadata.table_name);
    let actual = metadata.columns.len();
    let expected = metadata.column_count;

    if actual == expected {
        PreCheckResult {
            check_name,
            passed: true,
            message: format!("字段数量匹配: {} 个字段", actual),
            severity: "info".to_string(),
        }
    } else {
        PreCheckResult {
            check_name,
            passed: false,
            message: format!(
                "字段数量不匹配: 定义了 {} 个字段, COLUMNCOUNT 声明 {} 个",
                actual, expected
            ),
            severity: "error".to_string(),
        }
    }
}

/// 检查字段位置范围：连续、无重叠、最后一个字段结束位置等于 ROWLENGTH。
fn check_column_positions(metadata: &FlgMetadata) -> PreCheckResult {
    let check_name = format!("[{}] 字段位置范围", metadata.table_name);

    if metadata.columns.is_empty() {
        return PreCheckResult {
            check_name,
            passed: false,
            message: "没有字段定义，无法检查位置范围".to_string(),
            severity: "error".to_string(),
        };
    }

    // 按 start_pos 排序检查
    let mut sorted_cols = metadata.columns.clone();
    sorted_cols.sort_by_key(|c| c.start_pos);

    let mut issues: Vec<String> = Vec::new();

    // 检查第一个字段是否从位置1开始
    if sorted_cols[0].start_pos != 1 {
        issues.push(format!(
            "第一个字段 {} 起始位置为 {}，应为 1",
            sorted_cols[0].name, sorted_cols[0].start_pos
        ));
    }

    // 检查连续性（无间隙、无重叠）
    for i in 1..sorted_cols.len() {
        let prev_end = sorted_cols[i - 1].end_pos;
        let curr_start = sorted_cols[i].start_pos;
        let expected_start = prev_end + 1;

        if curr_start != expected_start {
            if curr_start < expected_start {
                issues.push(format!(
                    "字段 {} 与前一字段 {} 存在重叠: 前一字段结束于 {}, 当前字段起始于 {}",
                    sorted_cols[i].name,
                    sorted_cols[i - 1].name,
                    prev_end,
                    curr_start
                ));
            } else {
                issues.push(format!(
                    "字段 {} 与前一字段 {} 之间存在间隙: 前一字段结束于 {}, 当前字段起始于 {}",
                    sorted_cols[i].name,
                    sorted_cols[i - 1].name,
                    prev_end,
                    curr_start
                ));
            }
        }
    }

    // 检查最后一个字段的结束位置是否等于 ROWLENGTH
    let last_end = sorted_cols.last().unwrap().end_pos;
    if last_end != metadata.row_length {
        issues.push(format!(
            "最后一个字段结束位置为 {}, 但 ROWLENGTH 为 {}",
            last_end, metadata.row_length
        ));
    }

    if issues.is_empty() {
        PreCheckResult {
            check_name,
            passed: true,
            message: format!(
                "字段位置范围连续且完整: 共 {} 个字段, ROWLENGTH={}",
                metadata.columns.len(),
                metadata.row_length
            ),
            severity: "info".to_string(),
        }
    } else {
        PreCheckResult {
            check_name,
            passed: false,
            message: format!("字段位置范围异常: {}", issues.join("; ")),
            severity: "error".to_string(),
        }
    }
}

/// 检查 GZIP 文件完整性（尝试解压前几行验证）。
async fn check_gzip_integrity(pair: &GutFilePair) -> PreCheckResult {
    let check_name = format!("[{}] GZIP完整性", pair.table_name);
    let dat_path = Path::new(&pair.dat_path);

    if !dat_path.exists() {
        return PreCheckResult {
            check_name,
            passed: false,
            message: format!("GZIP文件不存在: {}", dat_path.display()),
            severity: "error".to_string(),
        };
    }

    // 尝试打开并解压前几行
    match try_decompress_lines(dat_path, 5) {
        Ok(lines_read) => PreCheckResult {
            check_name,
            passed: true,
            message: format!("GZIP文件可正常解压, 已验证前 {} 行", lines_read),
            severity: "info".to_string(),
        },
        Err(e) => PreCheckResult {
            check_name,
            passed: false,
            message: format!("GZIP文件解压失败: {}", e),
            severity: "error".to_string(),
        },
    }
}

/// 尝试解压 GZIP 文件的前 N 行，验证文件完整性。
fn try_decompress_lines(path: &Path, max_lines: usize) -> anyhow::Result<usize> {
    let file = File::open(path).map_err(|e| anyhow::anyhow!("打开文件失败: {}", e))?;
    let decoder = GzDecoder::new(file);
    let reader = BufReader::new(decoder);

    let mut count = 0;
    for line_result in reader.lines() {
        match line_result {
            Ok(_) => {
                count += 1;
                if count >= max_lines {
                    break;
                }
            }
            Err(e) => {
                return Err(anyhow::anyhow!("解压第 {} 行时出错: {}", count + 1, e));
            }
        }
    }

    Ok(count)
}

/// 检查实际行数与 ROWCOUNT 是否匹配。
async fn check_row_count(pair: &GutFilePair, metadata: &FlgMetadata) -> PreCheckResult {
    let check_name = format!("[{}] 行数匹配", pair.table_name);
    let dat_path = Path::new(&pair.dat_path);

    if !dat_path.exists() {
        return PreCheckResult {
            check_name,
            passed: false,
            message: format!("DAT文件不存在，无法检查行数: {}", dat_path.display()),
            severity: "warning".to_string(),
        };
    }

    // 流式解压计数行数
    match count_lines_in_gzip(dat_path) {
        Ok(actual_count) => {
            let expected = metadata.row_count;
            let passed = actual_count == expected;
            let message = if passed {
                format!("行数匹配: 实际 {} 行, 声明 {} 行", actual_count, expected)
            } else {
                format!("行数不匹配: 实际 {} 行, 声明 {} 行", actual_count, expected)
            };

            if !passed {
                warn!("[{}] {}", pair.table_name, message);
            } else {
                debug!("[{}] {}", pair.table_name, message);
            }

            PreCheckResult {
                check_name,
                passed,
                message,
                severity: "warning".to_string(),
            }
        }
        Err(e) => {
            warn!("[{}] 计数行数失败: {}", pair.table_name, e);
            PreCheckResult {
                check_name,
                passed: false,
                message: format!("计数行数失败: {}", e),
                severity: "warning".to_string(),
            }
        }
    }
}

/// 流式解压 GZIP 文件并计数非空行数。
fn count_lines_in_gzip(path: &Path) -> anyhow::Result<usize> {
    let file = File::open(path).map_err(|e| anyhow::anyhow!("打开文件失败: {}", e))?;
    let decoder = GzDecoder::new(file);
    let mut reader = BufReader::with_capacity(64 * 1024, decoder);

    let mut count: usize = 0;
    let mut buf: Vec<u8> = Vec::with_capacity(1024);

    loop {
        buf.clear();
        let n = reader
            .read_until(b'\n', &mut buf)
            .map_err(|e| anyhow::anyhow!("读取文件出错: {}", e))?;
        if n == 0 {
            break;
        }
        // 去除尾部换行符
        if buf.last() == Some(&b'\n') {
            buf.pop();
        }
        if buf.last() == Some(&b'\r') {
            buf.pop();
        }
        // 非空行计数
        if !buf.is_empty() {
            count += 1;
        }
    }

    Ok(count)
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

    #[tokio::test]
    async fn test_check_all_files_passes() {
        let results = check_all_files(&example_dir()).await;
        for result in &results {
            assert!(
                result.passed,
                "检查项 '{}' 未通过: {}",
                result.check_name, result.message
            );
        }
    }

    #[test]
    fn test_check_directory_exists() {
        let result = check_directory_exists(&example_dir());
        assert!(result.passed, "目录存在性检查应通过: {}", result.message);

        let result = check_directory_exists(Path::new("/nonexistent/path"));
        assert!(!result.passed, "不存在的目录应不通过");
    }

    #[test]
    fn test_check_file_pairs_found() {
        let pairs = parser::scan_directory(&example_dir()).unwrap();
        let result = check_file_pairs_found(&pairs);
        assert!(result.passed, "文件对检查应通过: {}", result.message);

        let result = check_file_pairs_found(&[]);
        assert!(!result.passed, "空文件对应不通过");
    }

    #[tokio::test]
    async fn test_check_single_file_pair() {
        let pairs = parser::scan_directory(&example_dir()).unwrap();
        for pair in &pairs {
            let results = check_single_file_pair(pair).await;
            for result in &results {
                assert!(
                    result.passed,
                    "[{}] 检查项 '{}' 未通过: {}",
                    pair.table_name, result.check_name, result.message
                );
            }
        }
    }

    #[test]
    fn test_check_filename_format() {
        let pairs = parser::scan_directory(&example_dir()).unwrap();
        for pair in &pairs {
            let result = check_filename_format(pair);
            assert!(
                result.passed,
                "[{}] 文件命名格式检查未通过: {}",
                pair.table_name, result.message
            );
        }
    }

    #[test]
    fn test_check_flg_parseable() {
        let pairs = parser::scan_directory(&example_dir()).unwrap();
        for pair in &pairs {
            let (result, meta) = check_flg_parseable(pair);
            assert!(
                result.passed,
                "[{}] FLG解析检查未通过: {}",
                pair.table_name, result.message
            );
            assert!(meta.is_some(), "[{}] 应返回元数据", pair.table_name);
        }
    }

    #[test]
    fn test_check_column_count() {
        let pairs = parser::scan_directory(&example_dir()).unwrap();
        for pair in &pairs {
            let (_, meta) = check_flg_parseable(pair);
            let meta = meta.unwrap();
            let result = check_column_count(&meta);
            assert!(
                result.passed,
                "[{}] 字段数量检查未通过: {}",
                pair.table_name, result.message
            );
        }
    }

    #[test]
    fn test_check_column_positions() {
        let pairs = parser::scan_directory(&example_dir()).unwrap();
        for pair in &pairs {
            let (_, meta) = check_flg_parseable(pair);
            let meta = meta.unwrap();
            let result = check_column_positions(&meta);
            assert!(
                result.passed,
                "[{}] 字段位置范围检查未通过: {}",
                pair.table_name, result.message
            );
        }
    }

    #[tokio::test]
    async fn test_check_gzip_integrity() {
        let pairs = parser::scan_directory(&example_dir()).unwrap();
        for pair in &pairs {
            let result = check_gzip_integrity(pair).await;
            assert!(
                result.passed,
                "[{}] GZIP完整性检查未通过: {}",
                pair.table_name, result.message
            );
        }
    }

    #[tokio::test]
    async fn test_check_row_count() {
        let pairs = parser::scan_directory(&example_dir()).unwrap();
        for pair in &pairs {
            let (_, meta) = check_flg_parseable(pair);
            let meta = meta.unwrap();
            let result = check_row_count(pair, &meta).await;
            assert!(
                result.passed,
                "[{}] 行数匹配检查未通过: {}",
                pair.table_name, result.message
            );
        }
    }
}
