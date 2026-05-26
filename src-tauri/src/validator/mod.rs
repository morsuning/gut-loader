//! 前置校验模块：磁盘空间、文件格式等入库前置检查。

pub mod disk;
pub mod file;

use std::path::Path;

use crate::models::PreCheckResult;

/// 执行所有前置检查。
///
/// - `directory`：数据文件所在目录
/// - `target_path`：目标路径（数据库所在磁盘或临时目录），用于检查目标磁盘空间
pub async fn run_all_checks(directory: &Path, target_path: Option<&Path>) -> Vec<PreCheckResult> {
    let mut results = Vec::new();

    // 1. 磁盘空间检查
    results.push(disk::check_disk_space(directory).await);
    if let Some(target) = target_path {
        results.push(disk::check_target_disk_space(target).await);
    }

    // 2. 文件格式检查
    let file_results = file::check_all_files(directory).await;
    results.extend(file_results);

    results
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
    async fn test_run_all_checks_passes() {
        let results = run_all_checks(&example_dir(), None).await;
        assert!(!results.is_empty(), "应返回至少一个检查结果");
        for result in &results {
            assert!(
                result.passed,
                "检查项 '{}' 未通过: {}",
                result.check_name, result.message
            );
        }
    }

    #[tokio::test]
    async fn test_run_all_checks_with_target() {
        let dir = example_dir();
        let results = run_all_checks(&dir, Some(&dir)).await;
        assert!(!results.is_empty(), "应返回至少一个检查结果");
        for result in &results {
            assert!(
                result.passed,
                "检查项 '{}' 未通过: {}",
                result.check_name, result.message
            );
        }
    }

    #[tokio::test]
    async fn test_run_all_checks_nonexistent_directory() {
        let results = run_all_checks(Path::new("/nonexistent/path/data"), None).await;
        // 磁盘空间检查可能失败，目录存在性检查一定失败
        let dir_check = results.iter().find(|r| r.check_name == "目录存在性");
        assert!(dir_check.is_some(), "应包含目录存在性检查");
        assert!(!dir_check.unwrap().passed, "不存在的目录应不通过");
    }
}
