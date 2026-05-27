//! 磁盘空间预校验。

use std::path::Path;
use std::process::Command;

use tracing::{debug, warn};

use crate::models::PreCheckResult;

/// 检查源目录所在磁盘的可用空间。
///
/// 规则：可用空间至少需要是数据文件总大小的 2 倍（解压缓冲）。
pub async fn check_disk_space(directory: &Path) -> PreCheckResult {
    let check_name = "磁盘空间(源)".to_string();

    // 计算目录下所有 .dat.gz 文件的总大小
    let total_data_size = match calculate_data_size(directory) {
        Ok(size) => size,
        Err(e) => {
            warn!("计算数据文件大小失败: {}", e);
            return PreCheckResult {
                check_name,
                passed: false,
                message: format!("计算数据文件大小失败: {}", e),
                severity: "warning".to_string(),
            };
        }
    };

    // 获取磁盘可用空间
    let available = match get_available_space(directory) {
        Ok(space) => space,
        Err(e) => {
            warn!("获取磁盘可用空间失败: {}", e);
            return PreCheckResult {
                check_name,
                passed: false,
                message: format!("获取磁盘可用空间失败: {}", e),
                severity: "warning".to_string(),
            };
        }
    };

    let required = total_data_size * 2;
    let passed = available >= required;

    let message = if passed {
        format!(
            "磁盘空间充足: 可用 {}, 数据文件总大小 {}, 需要至少 {}",
            format_bytes(available),
            format_bytes(total_data_size),
            format_bytes(required)
        )
    } else {
        format!(
            "磁盘空间不足: 可用 {}, 数据文件总大小 {}, 需要至少 {}",
            format_bytes(available),
            format_bytes(total_data_size),
            format_bytes(required)
        )
    };

    debug!("{}", message);

    PreCheckResult {
        check_name,
        passed,
        message,
        severity: "warning".to_string(),
    }
}

/// 检查目标路径（数据库/临时目录）磁盘空间。
///
/// 规则：至少需要 1GB 可用空间。
pub async fn check_target_disk_space(target: &Path) -> PreCheckResult {
    let check_name = "磁盘空间(目标)".to_string();
    let min_required: u64 = 1024 * 1024 * 1024; // 1GB

    let available = match get_available_space(target) {
        Ok(space) => space,
        Err(e) => {
            warn!("获取目标磁盘可用空间失败: {}", e);
            return PreCheckResult {
                check_name,
                passed: false,
                message: format!("获取目标磁盘可用空间失败: {}", e),
                severity: "warning".to_string(),
            };
        }
    };

    let passed = available >= min_required;

    let message = if passed {
        format!(
            "目标磁盘空间充足: 可用 {}, 最低要求 1GB",
            format_bytes(available)
        )
    } else {
        format!(
            "目标磁盘空间不足: 可用 {}, 最低要求 1GB",
            format_bytes(available)
        )
    };

    debug!("{}", message);

    PreCheckResult {
        check_name,
        passed,
        message,
        severity: "warning".to_string(),
    }
}

/// 计算目录下所有 .dat.gz 文件的总大小（字节）。
fn calculate_data_size(directory: &Path) -> anyhow::Result<u64> {
    let mut total: u64 = 0;
    let entries =
        std::fs::read_dir(directory).map_err(|e| anyhow::anyhow!("读取目录失败: {}", e))?;

    for entry in entries {
        let entry = entry.map_err(|e| anyhow::anyhow!("枚举目录条目失败: {}", e))?;
        let path = entry.path();
        if path.is_file() {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.ends_with(".dat.gz") {
                    let meta = std::fs::metadata(&path)
                        .map_err(|e| anyhow::anyhow!("获取文件元数据失败: {}", e))?;
                    total += meta.len();
                }
            }
        }
    }

    Ok(total)
}

/// 获取指定路径所在磁盘的可用空间（字节）。
///
/// - Unix/macOS: 通过 `df -k` 命令
/// - Windows: 通过 `GetDiskFreeSpaceExW` API
fn get_available_space(path: &Path) -> anyhow::Result<u64> {
    // 确保路径存在，如果不存在则使用父目录
    let check_path = if path.exists() {
        path.to_path_buf()
    } else {
        path.parent().unwrap_or(Path::new("/")).to_path_buf()
    };

    #[cfg(target_os = "windows")]
    {
        get_available_space_windows(&check_path)
    }

    #[cfg(not(target_os = "windows"))]
    {
        get_available_space_unix(&check_path)
    }
}

/// Unix/macOS: 通过 `df -k` 命令获取可用空间。
#[cfg(not(target_os = "windows"))]
fn get_available_space_unix(path: &Path) -> anyhow::Result<u64> {
    let output = Command::new("df")
        .arg("-k")
        .arg(path)
        .output()
        .map_err(|e| anyhow::anyhow!("执行 df 命令失败: {}", e))?;

    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "df 命令返回错误: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    if lines.len() < 2 {
        return Err(anyhow::anyhow!("df 输出格式异常: {}", stdout));
    }

    let data_line = lines[1];
    let fields: Vec<&str> = data_line.split_whitespace().collect();
    // macOS 的 df -k 输出有 9 列，Available 在第 4 列（索引 3）
    // Linux 的 df -k 输出有 6 列，Available 在第 4 列（索引 3）
    if fields.len() < 4 {
        return Err(anyhow::anyhow!("df 输出字段不足: {:?}", fields));
    }

    let available_kb: u64 = fields[3]
        .parse()
        .map_err(|e| anyhow::anyhow!("解析可用空间失败 '{}': {}", fields[3], e))?;

    Ok(available_kb * 1024) // 转换为字节
}

/// Windows: 通过 `GetDiskFreeSpaceExW` API 获取可用空间。
#[cfg(target_os = "windows")]
fn get_available_space_windows(path: &Path) -> anyhow::Result<u64> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

    // 转换为 null 结尾的宽字符串
    let wide_path: Vec<u16> = path.as_os_str().encode_wide().chain(std::iter::once(0)).collect();

    let mut free_bytes_available: u64 = 0;

    let result = unsafe {
        GetDiskFreeSpaceExW(
            wide_path.as_ptr(),
            &mut free_bytes_available as *mut u64,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };

    if result == 0 {
        return Err(anyhow::anyhow!(
            "GetDiskFreeSpaceExW 调用失败，路径: {}",
            path.display()
        ));
    }

    Ok(free_bytes_available)
}

/// 将字节大小格式化为人类可读格式。
fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
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
    async fn test_check_disk_space_passes() {
        let result = check_disk_space(&example_dir()).await;
        assert!(result.passed, "磁盘空间检查应通过: {}", result.message);
        assert_eq!(result.severity, "warning");
    }

    #[tokio::test]
    async fn test_check_target_disk_space_passes() {
        let result = check_target_disk_space(&example_dir()).await;
        assert!(result.passed, "目标磁盘空间检查应通过: {}", result.message);
        assert_eq!(result.severity, "warning");
    }

    #[test]
    fn test_calculate_data_size() {
        let size = calculate_data_size(&example_dir()).unwrap();
        assert!(size > 0, "数据文件总大小应大于 0");
    }

    #[test]
    fn test_get_available_space() {
        let space = get_available_space(&example_dir()).unwrap();
        assert!(space > 0, "可用空间应大于 0");
    }

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(500), "500 B");
        assert_eq!(format_bytes(1024), "1.00 KB");
        assert_eq!(format_bytes(1048576), "1.00 MB");
        assert_eq!(format_bytes(1073741824), "1.00 GB");
    }
}
