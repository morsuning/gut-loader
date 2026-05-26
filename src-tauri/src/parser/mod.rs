//! 文件解析模块：负责 GUT 双文件标准（dat + flg）的解析。

pub mod dat;
pub mod flg;

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use tracing::{debug, warn};

use crate::models::GutFilePair;

/// 扫描指定目录，按命名规则配对 .flg 与 .dat.gz 文件。
///
/// 命名规则：`<TABLE_NAME>.<YYYYMMDD>.<HHMMSS>.<SEQUENCE>.<EXTENSION>`，
/// 例如：`employee.20260421.000000.0000.flg` 与 `employee.20260421.000000.0000.dat.gz`。
///
/// 仅当同一前缀（`TABLE_NAME.YYYYMMDD.HHMMSS.SEQUENCE`）下同时存在 .flg 与 .dat.gz
/// 时才会返回该文件对；返回结果按表名升序排序，便于稳定处理。
pub fn scan_directory(dir: &Path) -> Result<Vec<GutFilePair>> {
    if !dir.is_dir() {
        return Err(anyhow::anyhow!("扫描路径不是目录: {}", dir.display()));
    }

    /// 按文件名前缀分组的中间结构。
    #[derive(Default)]
    struct Slot {
        flg: Option<PathBuf>,
        dat: Option<PathBuf>,
    }

    let mut slots: HashMap<String, Slot> = HashMap::new();

    for entry in fs::read_dir(dir).with_context(|| format!("读取目录失败: {}", dir.display()))?
    {
        let entry = entry.with_context(|| format!("枚举目录条目失败: {}", dir.display()))?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let name = match path.file_name().and_then(|s| s.to_str()) {
            Some(s) => s.to_string(),
            None => continue,
        };

        if let Some(stem) = name.strip_suffix(".flg") {
            slots.entry(stem.to_string()).or_default().flg = Some(path);
        } else if let Some(stem) = name.strip_suffix(".dat.gz") {
            slots.entry(stem.to_string()).or_default().dat = Some(path);
        } else {
            debug!("跳过非 GUT 文件: {}", name);
        }
    }

    let mut pairs: Vec<GutFilePair> = Vec::new();
    for (stem, slot) in slots {
        match (slot.flg, slot.dat) {
            (Some(flg), Some(dat)) => {
                let parts: Vec<&str> = stem.splitn(4, '.').collect();
                if parts.len() != 4 {
                    warn!("文件名不符合 GUT 命名规范: {}", stem);
                    continue;
                }
                pairs.push(GutFilePair {
                    table_name: parts[0].to_string(),
                    date: parts[1].to_string(),
                    time: parts[2].to_string(),
                    sequence: parts[3].to_string(),
                    flg_path: flg.to_string_lossy().into_owned(),
                    dat_path: dat.to_string_lossy().into_owned(),
                });
            }
            (Some(_), None) => {
                warn!("仅找到 .flg 缺少配对的 .dat.gz: {}", stem);
            }
            (None, Some(_)) => {
                warn!("仅找到 .dat.gz 缺少配对的 .flg: {}", stem);
            }
            (None, None) => {}
        }
    }

    pairs.sort_by(|a, b| {
        a.table_name
            .cmp(&b.table_name)
            .then_with(|| a.date.cmp(&b.date))
            .then_with(|| a.time.cmp(&b.time))
            .then_with(|| a.sequence.cmp(&b.sequence))
    });

    Ok(pairs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn example_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("example_data")
    }

    #[test]
    fn test_scan_directory_finds_five_pairs() {
        let pairs = scan_directory(&example_dir()).expect("扫描 example_data 失败");
        assert_eq!(pairs.len(), 5, "应当发现 5 组文件对，实际 {}", pairs.len());
        let names: Vec<&str> = pairs.iter().map(|p| p.table_name.as_str()).collect();
        assert!(names.contains(&"employee"));
        assert!(names.contains(&"order"));
        assert!(names.contains(&"product"));
        assert!(names.contains(&"transaction"));
        assert!(names.contains(&"user"));

        for p in &pairs {
            assert!(
                Path::new(&p.flg_path).exists(),
                "flg 路径不存在: {}",
                p.flg_path
            );
            assert!(
                Path::new(&p.dat_path).exists(),
                "dat 路径不存在: {}",
                p.dat_path
            );
            assert_eq!(p.date, "20260421");
            assert_eq!(p.time, "000000");
            assert_eq!(p.sequence, "0000");
        }
    }
}
