//! 批量加载策略与执行实现。
//!
//! 负责将解析后的数据按批次写入目标数据库，支持进度回调和错误容忍。
//!
//! 加载策略：
//! - 小文件（<= [`STREAMING_THRESHOLD_BYTES`]）：一次性解析全部行后再分批入库，
//!   小文件场景下减少 IO 与系统调用开销。
//! - 大文件（> [`STREAMING_THRESHOLD_BYTES`]）：流式读取 dat.gz，每凑齐一批立即
//!   写库，内存占用始终控制在单批数据量级，避免大文件解析时内存暴涨。

use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::time::Instant;

use anyhow::{Context, Result};
use flate2::read::GzDecoder;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use crate::database::{safe_batch_size, DatabaseLoader};
use crate::models::{DataRow, FlgMetadata, LoadProgress, TableReport};
use crate::parser;

/// 默认批次大小
const DEFAULT_BATCH_SIZE: usize = 1000;

/// 启用流式加载的文件大小阈值（字节，默认 100 MB）。
///
/// 当 dat.gz 实际磁盘大小超过该阈值时，会自动切换到 [`load_table_streaming`]，
/// 否则使用一次性内存加载 [`load_table_inmemory`]。
pub const STREAMING_THRESHOLD_BYTES: u64 = 100 * 1024 * 1024;

/// 加载单张表的数据到目标数据库（自动选择策略）。
///
/// 根据 dat 文件大小自动选择：
/// - 小文件（<= 100MB）：调用 [`load_table_inmemory`]
/// - 大文件（> 100MB）：调用 [`load_table_streaming`]
pub async fn load_table(
    loader: &dyn DatabaseLoader,
    flg_path: &Path,
    dat_path: &Path,
    progress_tx: Option<mpsc::Sender<LoadProgress>>,
) -> Result<TableReport> {
    let file_size = std::fs::metadata(dat_path)
        .with_context(|| format!("读取 dat 文件元数据失败: {}", dat_path.display()))?
        .len();

    if file_size > STREAMING_THRESHOLD_BYTES {
        info!(
            "文件 {} 大小 {} MB 超过阈值 {} MB，使用流式加载",
            dat_path.display(),
            file_size / 1024 / 1024,
            STREAMING_THRESHOLD_BYTES / 1024 / 1024,
        );
        load_table_streaming(loader, flg_path, dat_path, progress_tx).await
    } else {
        debug!(
            "文件 {} 大小 {} 字节，使用内存加载",
            dat_path.display(),
            file_size
        );
        load_table_inmemory(loader, flg_path, dat_path, progress_tx).await
    }
}

/// 一次性内存加载：先 `parse_dat` 全部行，再分批 `batch_insert`。
///
/// 适合小文件，能够利用一次解析完成的局部性，并在出错时给出更准确的整体行数。
pub async fn load_table_inmemory(
    loader: &dyn DatabaseLoader,
    flg_path: &Path,
    dat_path: &Path,
    progress_tx: Option<mpsc::Sender<LoadProgress>>,
) -> Result<TableReport> {
    let start = Instant::now();

    let metadata = parser::flg::parse_flg(flg_path)
        .with_context(|| format!("解析 flg 文件失败: {}", flg_path.display()))?;
    let table_name = metadata.table_name.clone();
    let total_rows = metadata.row_count;

    info!("开始加载表 {} ({} 行)", table_name, total_rows);

    loader
        .create_table(&metadata)
        .await
        .with_context(|| format!("创建表 {} 失败", table_name))?;

    let existing_rows: usize = loader.get_row_count(&table_name).await.unwrap_or_default();

    if existing_rows > 0 {
        info!(
            "表 {} 已有 {} 行数据（断点续传）",
            table_name, existing_rows
        );
    }

    let batch_size = safe_batch_size(metadata.columns.len(), DEFAULT_BATCH_SIZE);

    let mut total_success: usize = 0;
    let mut total_failed: usize = 0;
    let mut errors: Vec<String> = Vec::new();
    let mut rows_skipped: usize = 0;

    let all_rows = parser::dat::parse_dat(dat_path, &metadata)
        .with_context(|| format!("解析 dat 文件失败: {}", dat_path.display()))?;

    let rows_to_load: &[DataRow] = if existing_rows > 0 && existing_rows < all_rows.len() {
        rows_skipped = existing_rows;
        &all_rows[existing_rows..]
    } else if existing_rows >= all_rows.len() {
        info!("表 {} 数据已全部加载，跳过", table_name);
        rows_skipped = all_rows.len();
        &[]
    } else {
        &all_rows
    };

    for chunk in rows_to_load.chunks(batch_size) {
        match loader.batch_insert(&table_name, &metadata, chunk).await {
            Ok(n) => {
                total_success += n;
            }
            Err(e) => {
                let err_msg = format!("批量插入表 {} 失败: {}", table_name, e);
                error!("{}", err_msg);
                total_failed += chunk.len();
                errors.push(err_msg);
            }
        }

        send_progress(
            &progress_tx,
            &table_name,
            total_rows,
            total_success + rows_skipped,
            total_failed,
            &start,
        );
    }

    let elapsed = start.elapsed().as_millis() as u64;
    let total_loaded = total_success + rows_skipped;
    let speed = compute_speed(total_loaded, elapsed);

    send_final_progress(
        &progress_tx,
        &table_name,
        total_rows,
        total_loaded,
        total_failed,
        elapsed,
        speed,
    );

    info!(
        "表 {} 内存加载完成: 成功 {} 行, 失败 {} 行, 耗时 {}ms, 速度 {:.0} 行/秒",
        table_name, total_success, total_failed, elapsed, speed
    );

    Ok(TableReport {
        table_name,
        row_count: total_rows,
        success_count: total_loaded,
        failed_count: total_failed,
        elapsed_ms: elapsed,
        speed,
        errors,
    })
}

/// 流式加载：边读 dat.gz 边按批写库，内存占用控制在单批数据量级。
///
/// 实现要点：
/// 1. 使用 `GzDecoder + BufReader` 流式解压；
/// 2. 逐行 `read_until('\n')` 解析为 [`DataRow`]，每凑齐 `batch_size` 行立即调用
///    [`DatabaseLoader::batch_insert`] 入库并清空本地缓冲；
/// 3. 末尾不满一批的数据在循环结束后单独入库；
/// 4. 支持断点续传（按已存在行数跳过开头若干行）；
/// 5. 解析阶段的单行错误只记录日志、不中断后续行。
pub async fn load_table_streaming(
    loader: &dyn DatabaseLoader,
    flg_path: &Path,
    dat_path: &Path,
    progress_tx: Option<mpsc::Sender<LoadProgress>>,
) -> Result<TableReport> {
    let start = Instant::now();

    let metadata = parser::flg::parse_flg(flg_path)
        .with_context(|| format!("解析 flg 文件失败: {}", flg_path.display()))?;
    let table_name = metadata.table_name.clone();
    let total_rows_expected = metadata.row_count;

    info!(
        "开始流式加载表 {} (期望 {} 行)",
        table_name, total_rows_expected
    );

    loader
        .create_table(&metadata)
        .await
        .with_context(|| format!("创建表 {} 失败", table_name))?;

    let existing_rows = loader.get_row_count(&table_name).await.unwrap_or(0);
    if existing_rows > 0 {
        info!(
            "表 {} 已有 {} 行数据（断点续传），将跳过前 {} 行",
            table_name, existing_rows, existing_rows
        );
    }

    let batch_size = safe_batch_size(metadata.columns.len(), DEFAULT_BATCH_SIZE);

    // 打开文件并构造解压/缓冲读取器
    let file = File::open(dat_path)
        .with_context(|| format!("打开 dat 文件失败: {}", dat_path.display()))?;
    let decoded: Box<dyn Read + Send> = if parser::dat::is_gzip(dat_path) {
        Box::new(GzDecoder::new(file))
    } else {
        Box::new(file)
    };
    let mut reader = BufReader::with_capacity(64 * 1024, decoded);

    let mut total_success: usize = 0;
    let mut total_failed: usize = 0;
    let mut errors: Vec<String> = Vec::new();
    let mut rows_read: usize = 0;
    let mut rows_skipped: usize = 0;

    let mut batch: Vec<DataRow> = Vec::with_capacity(batch_size);
    let mut line_buf: Vec<u8> = Vec::with_capacity(metadata.row_length + 2);

    loop {
        line_buf.clear();
        let n = reader
            .read_until(b'\n', &mut line_buf)
            .with_context(|| format!("读取 dat 文件出错: {}", dat_path.display()))?;
        if n == 0 {
            break;
        }
        if line_buf.last() == Some(&b'\n') {
            line_buf.pop();
        }
        if line_buf.last() == Some(&b'\r') {
            line_buf.pop();
        }
        if line_buf.is_empty() {
            continue;
        }

        rows_read += 1;

        // 断点续传：跳过已加载的前若干行
        if rows_read <= existing_rows {
            rows_skipped += 1;
            continue;
        }

        match parser::dat::parse_row_bytes(&line_buf, &metadata, rows_read) {
            Ok(row) => batch.push(row),
            Err(err) => {
                warn!("第 {} 行解析失败: {}", rows_read, err);
                continue;
            }
        }

        if batch.len() >= batch_size {
            flush_batch(
                loader,
                &table_name,
                &metadata,
                &batch,
                rows_read,
                &mut total_success,
                &mut total_failed,
                &mut errors,
            )
            .await;

            send_progress(
                &progress_tx,
                &table_name,
                total_rows_expected,
                total_success + rows_skipped,
                total_failed,
                &start,
            );
            batch.clear();
        }
    }

    // 处理末尾不满一批的剩余数据
    if !batch.is_empty() {
        flush_batch(
            loader,
            &table_name,
            &metadata,
            &batch,
            rows_read,
            &mut total_success,
            &mut total_failed,
            &mut errors,
        )
        .await;
        batch.clear();
    }

    let elapsed = start.elapsed().as_millis() as u64;
    let total_loaded = total_success + rows_skipped;
    let speed = compute_speed(total_loaded, elapsed);

    send_final_progress(
        &progress_tx,
        &table_name,
        total_rows_expected,
        total_loaded,
        total_failed,
        elapsed,
        speed,
    );

    info!(
        "表 {} 流式加载完成: 成功 {} 行, 失败 {} 行, 跳过 {} 行, 耗时 {}ms, 速度 {:.0} 行/秒",
        table_name, total_success, total_failed, rows_skipped, elapsed, speed
    );

    Ok(TableReport {
        table_name,
        row_count: rows_read,
        success_count: total_loaded,
        failed_count: total_failed,
        elapsed_ms: elapsed,
        speed,
        errors,
    })
}

/// 真正调用 `batch_insert` 并更新计数 / 错误列表。
#[allow(clippy::too_many_arguments)]
async fn flush_batch(
    loader: &dyn DatabaseLoader,
    table_name: &str,
    metadata: &FlgMetadata,
    batch: &[DataRow],
    rows_read: usize,
    total_success: &mut usize,
    total_failed: &mut usize,
    errors: &mut Vec<String>,
) {
    match loader.batch_insert(table_name, metadata, batch).await {
        Ok(n) => {
            *total_success += n;
        }
        Err(e) => {
            let err_msg = format!(
                "流式批量插入表 {} 失败 (累计行号 {}): {}",
                table_name, rows_read, e
            );
            error!("{}", err_msg);
            *total_failed += batch.len();
            errors.push(err_msg);
        }
    }
}

/// 发送加载中进度。
fn send_progress(
    progress_tx: &Option<mpsc::Sender<LoadProgress>>,
    table_name: &str,
    total_rows: usize,
    loaded_rows: usize,
    failed_rows: usize,
    start: &Instant,
) {
    if let Some(tx) = progress_tx {
        let elapsed = start.elapsed().as_millis() as u64;
        let speed = compute_speed(loaded_rows, elapsed);
        let progress = LoadProgress {
            table_name: table_name.to_string(),
            total_rows,
            loaded_rows,
            failed_rows,
            status: "loading".to_string(),
            speed,
            elapsed_ms: elapsed,
        };
        if tx.try_send(progress).is_err() {
            warn!("进度通道已满，跳过本次进度通知");
        }
    }
}

/// 发送最终完成进度。
fn send_final_progress(
    progress_tx: &Option<mpsc::Sender<LoadProgress>>,
    table_name: &str,
    total_rows: usize,
    loaded_rows: usize,
    failed_rows: usize,
    elapsed_ms: u64,
    speed: f64,
) {
    if let Some(tx) = progress_tx {
        let status = if failed_rows > 0 {
            "completed_with_errors"
        } else {
            "completed"
        };
        let final_progress = LoadProgress {
            table_name: table_name.to_string(),
            total_rows,
            loaded_rows,
            failed_rows,
            status: status.to_string(),
            speed,
            elapsed_ms,
        };
        let _ = tx.try_send(final_progress);
    }
}

#[inline]
fn compute_speed(rows: usize, elapsed_ms: u64) -> f64 {
    if elapsed_ms > 0 {
        rows as f64 / (elapsed_ms as f64 / 1000.0)
    } else {
        0.0
    }
}
