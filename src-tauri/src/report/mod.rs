//! 入库报告生成模块：聚合记录数、成功率、耗时、加载速率等指标。
//!
//! 提供 [`ReportGenerator`] 用于在加载过程中收集各表的执行结果，并在最终
//! 生成结构化的 [`LoadReport`]。同时支持 JSON 持久化与从文件加载。

use crate::models::{LoadReport, TableReport};
use anyhow::{Context, Result};
use chrono::Local;
use std::path::Path;
use tracing::{debug, info};

/// 报告生成器。
///
/// 在数据加载过程中累积各表的 [`TableReport`]，最终汇总为 [`LoadReport`]。
pub struct ReportGenerator {
    table_reports: Vec<TableReport>,
    start_time: std::time::Instant,
}

impl ReportGenerator {
    /// 创建新的报告生成器（计时起点为当前时刻）。
    pub fn new() -> Self {
        Self {
            table_reports: Vec::new(),
            start_time: std::time::Instant::now(),
        }
    }

    /// 添加单表报告。
    pub fn add_table_report(&mut self, report: TableReport) {
        debug!(
            table = %report.table_name,
            rows = report.row_count,
            success = report.success_count,
            failed = report.failed_count,
            "add table report"
        );
        self.table_reports.push(report);
    }

    /// 当前已收集的单表报告数量。
    pub fn table_count(&self) -> usize {
        self.table_reports.len()
    }

    /// 生成最终汇总报告。
    pub fn generate(&self) -> LoadReport {
        let total_tables = self.table_reports.len();
        let total_rows: usize = self.table_reports.iter().map(|r| r.row_count).sum();
        let success_rows: usize = self.table_reports.iter().map(|r| r.success_count).sum();
        let failed_rows: usize = self.table_reports.iter().map(|r| r.failed_count).sum();
        let success_rate = if total_rows > 0 {
            success_rows as f64 / total_rows as f64
        } else {
            0.0
        };
        let total_elapsed_ms = self.start_time.elapsed().as_millis() as u64;
        let avg_speed = if total_elapsed_ms > 0 {
            success_rows as f64 / (total_elapsed_ms as f64 / 1000.0)
        } else {
            0.0
        };

        LoadReport {
            total_tables,
            total_rows,
            success_rows,
            failed_rows,
            success_rate,
            total_elapsed_ms,
            avg_speed,
            table_reports: self.table_reports.clone(),
        }
    }

    /// 将报告导出为 JSON 文件，返回写入的 JSON 字符串。
    pub fn export_json(&self, output_path: &Path) -> Result<String> {
        let report = self.generate();
        let json =
            serde_json::to_string_pretty(&report).context("serialize LoadReport to JSON failed")?;
        if let Some(parent) = output_path.parent() {
            if !parent.as_os_str().is_empty() && !parent.exists() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("create dir {} failed", parent.display()))?;
            }
        }
        std::fs::write(output_path, &json)
            .with_context(|| format!("write report to {} failed", output_path.display()))?;
        info!(path = %output_path.display(), "report exported as JSON");
        Ok(json)
    }

    /// 将报告导出为格式化的文本摘要。
    pub fn export_summary(&self) -> String {
        let report = self.generate();
        let mut summary = String::new();

        summary.push_str("═══════════════════════════════════════════\n");
        summary.push_str("          GUT 数据加载报告\n");
        summary.push_str(&format!(
            "  生成时间: {}\n",
            Local::now().format("%Y-%m-%d %H:%M:%S")
        ));
        summary.push_str("═══════════════════════════════════════════\n\n");

        summary.push_str("【汇总统计】\n");
        summary.push_str(&format!("  加载表数: {}\n", report.total_tables));
        summary.push_str(&format!("  总记录数: {}\n", report.total_rows));
        summary.push_str(&format!("  成功记录: {}\n", report.success_rows));
        summary.push_str(&format!("  失败记录: {}\n", report.failed_rows));
        summary.push_str(&format!(
            "  成功率:   {:.2}%\n",
            report.success_rate * 100.0
        ));
        summary.push_str(&format!(
            "  总耗时:   {}\n",
            format_duration(report.total_elapsed_ms)
        ));
        summary.push_str(&format!("  平均速率: {:.0} 行/秒\n\n", report.avg_speed));

        summary.push_str("【各表详情】\n");
        summary.push_str(&format!(
            "{:<15} {:>8} {:>8} {:>8} {:>10} {:>12}\n",
            "表名", "总行数", "成功", "失败", "耗时", "速率(行/秒)"
        ));
        summary.push_str(&"-".repeat(65));
        summary.push('\n');

        for tr in &report.table_reports {
            summary.push_str(&format!(
                "{:<15} {:>8} {:>8} {:>8} {:>10} {:>12.0}\n",
                tr.table_name,
                tr.row_count,
                tr.success_count,
                tr.failed_count,
                format_duration(tr.elapsed_ms),
                tr.speed
            ));
        }

        let has_errors = report.table_reports.iter().any(|r| !r.errors.is_empty());
        if has_errors {
            summary.push_str("\n【错误详情】\n");
            for tr in &report.table_reports {
                if !tr.errors.is_empty() {
                    summary.push_str(&format!("\n  表 '{}':\n", tr.table_name));
                    for (i, err) in tr.errors.iter().enumerate().take(10) {
                        summary.push_str(&format!("    {}. {}\n", i + 1, err));
                    }
                    if tr.errors.len() > 10 {
                        summary
                            .push_str(&format!("    ... 及其他 {} 条错误\n", tr.errors.len() - 10));
                    }
                }
            }
        }

        summary.push_str("\n═══════════════════════════════════════════\n");
        summary
    }
}

impl Default for ReportGenerator {
    fn default() -> Self {
        Self::new()
    }
}

/// 从 JSON 文件加载已生成的报告。
pub fn load_report_from_file(path: &Path) -> Result<LoadReport> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("read report file {} failed", path.display()))?;
    let report: LoadReport =
        serde_json::from_str(&content).context("parse LoadReport JSON failed")?;
    Ok(report)
}

/// 格式化耗时（毫秒 -> 可读字符串）。
fn format_duration(ms: u64) -> String {
    if ms < 1000 {
        format!("{}ms", ms)
    } else if ms < 60_000 {
        format!("{:.1}s", ms as f64 / 1000.0)
    } else {
        let minutes = ms / 60_000;
        let seconds = (ms % 60_000) as f64 / 1000.0;
        format!("{}m{:.1}s", minutes, seconds)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_table_report(
        name: &str,
        total: usize,
        success: usize,
        errors: Vec<String>,
    ) -> TableReport {
        let failed = total - success;
        TableReport {
            table_name: name.to_string(),
            row_count: total,
            success_count: success,
            failed_count: failed,
            elapsed_ms: 1500,
            speed: success as f64 / 1.5,
            errors,
        }
    }

    #[test]
    fn test_generate_aggregates_correctly() {
        let mut gen = ReportGenerator::new();
        gen.add_table_report(make_table_report("user", 100, 95, vec![]));
        gen.add_table_report(make_table_report("order", 200, 180, vec!["e1".into()]));
        gen.add_table_report(make_table_report("product", 50, 50, vec![]));

        let report = gen.generate();
        assert_eq!(report.total_tables, 3);
        assert_eq!(report.total_rows, 350);
        assert_eq!(report.success_rows, 325);
        assert_eq!(report.failed_rows, 25);
        // success_rate = 325 / 350
        assert!((report.success_rate - (325.0 / 350.0)).abs() < 1e-9);
        assert_eq!(report.table_reports.len(), 3);
    }

    #[test]
    fn test_generate_empty() {
        let gen = ReportGenerator::new();
        let report = gen.generate();
        assert_eq!(report.total_tables, 0);
        assert_eq!(report.total_rows, 0);
        assert_eq!(report.success_rate, 0.0);
        assert_eq!(report.avg_speed, 0.0);
    }

    #[test]
    fn test_success_rate_full_success() {
        let mut gen = ReportGenerator::new();
        gen.add_table_report(make_table_report("t", 100, 100, vec![]));
        let report = gen.generate();
        assert!((report.success_rate - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_success_rate_all_failed() {
        let mut gen = ReportGenerator::new();
        gen.add_table_report(make_table_report("t", 100, 0, vec!["err".into()]));
        let report = gen.generate();
        assert_eq!(report.success_rate, 0.0);
        assert_eq!(report.failed_rows, 100);
    }

    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration(0), "0ms");
        assert_eq!(format_duration(500), "500ms");
        assert_eq!(format_duration(999), "999ms");
        assert_eq!(format_duration(1000), "1.0s");
        assert_eq!(format_duration(1500), "1.5s");
        assert_eq!(format_duration(59_999), "60.0s");
        assert_eq!(format_duration(60_000), "1m0.0s");
        assert_eq!(format_duration(75_500), "1m15.5s");
        assert_eq!(format_duration(125_000), "2m5.0s");
    }

    #[test]
    fn test_export_summary_contains_key_sections() {
        let mut gen = ReportGenerator::new();
        gen.add_table_report(make_table_report("user", 100, 100, vec![]));
        gen.add_table_report(make_table_report(
            "order",
            10,
            5,
            vec!["row 1 err".into(), "row 2 err".into()],
        ));

        let summary = gen.export_summary();
        assert!(summary.contains("GUT 数据加载报告"));
        assert!(summary.contains("【汇总统计】"));
        assert!(summary.contains("【各表详情】"));
        assert!(summary.contains("user"));
        assert!(summary.contains("order"));
        assert!(summary.contains("【错误详情】"));
        assert!(summary.contains("row 1 err"));
    }

    #[test]
    fn test_export_summary_no_errors_section_when_empty() {
        let mut gen = ReportGenerator::new();
        gen.add_table_report(make_table_report("user", 100, 100, vec![]));
        let summary = gen.export_summary();
        assert!(!summary.contains("【错误详情】"));
    }

    #[test]
    fn test_export_summary_truncates_errors() {
        let many_errors: Vec<String> = (0..15).map(|i| format!("err-{}", i)).collect();
        let mut gen = ReportGenerator::new();
        gen.add_table_report(make_table_report("big", 100, 85, many_errors));
        let summary = gen.export_summary();
        assert!(summary.contains("err-0"));
        assert!(summary.contains("err-9"));
        // 第 11 条之后被截断
        assert!(!summary.contains("err-10"));
        assert!(summary.contains("及其他 5 条错误"));
    }

    #[test]
    fn test_export_and_load_json_roundtrip() {
        let mut gen = ReportGenerator::new();
        gen.add_table_report(make_table_report("user", 100, 95, vec!["e".into()]));
        gen.add_table_report(make_table_report("order", 200, 200, vec![]));

        let tmp_dir =
            std::env::temp_dir().join(format!("gut-loader-report-test-{}", std::process::id()));
        std::fs::create_dir_all(&tmp_dir).unwrap();
        let path = tmp_dir.join("report.json");

        let json = gen.export_json(&path).unwrap();
        assert!(json.contains("total_tables"));
        assert!(path.exists());

        let loaded = load_report_from_file(&path).unwrap();
        assert_eq!(loaded.total_tables, 2);
        assert_eq!(loaded.total_rows, 300);
        assert_eq!(loaded.success_rows, 295);
        assert_eq!(loaded.failed_rows, 5);
        assert_eq!(loaded.table_reports.len(), 2);
        assert_eq!(loaded.table_reports[0].table_name, "user");

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&tmp_dir);
    }

    #[test]
    fn test_export_json_creates_parent_dir() {
        let tmp_dir =
            std::env::temp_dir().join(format!("gut-loader-report-mkdir-{}", std::process::id()));
        let nested = tmp_dir.join("nested").join("subdir");
        let path = nested.join("r.json");

        let mut gen = ReportGenerator::new();
        gen.add_table_report(make_table_report("t", 1, 1, vec![]));
        gen.export_json(&path).unwrap();
        assert!(path.exists());

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir_all(&tmp_dir);
    }

    #[test]
    fn test_default_equals_new() {
        let g = ReportGenerator::default();
        assert_eq!(g.table_count(), 0);
    }

    #[test]
    fn test_load_report_from_missing_file() {
        let path = std::env::temp_dir().join("definitely-not-exist-gut-report.json");
        let _ = std::fs::remove_file(&path);
        assert!(load_report_from_file(&path).is_err());
    }
}
