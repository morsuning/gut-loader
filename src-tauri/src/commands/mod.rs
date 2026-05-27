//! Tauri 命令入口模块：暴露给前端调用的命令集合。
//!
//! 本模块负责把后端各业务模块（parser/validator/database/llm/loader/report）
//! 通过 `#[tauri::command]` 宏暴露给前端 JS 层；同时维护跨命令共享的运行
//! 状态（[`AppState`]），并在批量加载过程中通过 Tauri 事件系统向前端推送
//! 实时进度、单表完成、整体完成与错误事件。

use std::path::PathBuf;
use std::sync::Arc;

use tauri::{AppHandle, Emitter, State};
use tokio::sync::Mutex;
use tracing::{error, info, warn};

use crate::database;
use crate::llm;
use crate::loader;
use crate::models::{DatabaseConfig, GutFilePair, LlmConfig, LoadReport, PreCheckResult};
use crate::parser;
use crate::report::ReportGenerator;
use crate::validator;

/// 跨命令共享的应用状态。
///
/// - `last_report`：保存最近一次完整加载任务汇总后的 [`LoadReport`]
/// - `is_loading`：标识当前是否处于加载执行阶段，防止并发触发
/// - `cancel_flag`：用于在长时间任务中协作式停止（前端调用 `stop_loading` 后置位）
#[derive(Default)]
pub struct AppState {
    pub last_report: Arc<Mutex<Option<LoadReport>>>,
    pub is_loading: Arc<Mutex<bool>>,
    pub cancel_flag: Arc<Mutex<bool>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            last_report: Arc::new(Mutex::new(None)),
            is_loading: Arc::new(Mutex::new(false)),
            cancel_flag: Arc::new(Mutex::new(false)),
        }
    }
}

// ---------------------------------------------------------------------------
// 文件扫描
// ---------------------------------------------------------------------------

/// 扫描目录识别 GUT 文件对（.flg + .dat.gz）。
#[tauri::command]
pub async fn scan_directory(path: String) -> Result<Vec<GutFilePair>, String> {
    let dir = PathBuf::from(&path);
    parser::scan_directory(&dir).map_err(|e| format!("扫描目录失败: {}", e))
}

// ---------------------------------------------------------------------------
// 前置检查
// ---------------------------------------------------------------------------

/// 执行前置检查（磁盘空间 + 文件格式 + 表结构等）。
///
/// `db_config` 当前未参与检查（保留参数为后续连通性检查留位），调用时仍需
/// 由前端提供以保持命令签名稳定。
#[tauri::command]
pub async fn run_pre_checks(
    path: String,
    db_config: DatabaseConfig,
) -> Result<Vec<PreCheckResult>, String> {
    let _ = db_config; // 当前未使用，保留参数以兼容前端协议
    let dir = PathBuf::from(&path);
    let results = validator::run_all_checks(&dir, None).await;
    Ok(results)
}

// ---------------------------------------------------------------------------
// 数据库连接
// ---------------------------------------------------------------------------

/// 测试数据库连通性。成功返回 true，失败返回详细错误信息。
#[tauri::command]
pub async fn test_connection(config: DatabaseConfig) -> Result<bool, String> {
    let loader = database::create_loader(&config)
        .await
        .map_err(|e| format!("创建数据库连接失败: {}", e))?;
    match loader.test_connection().await {
        Ok(ok) => {
            let _ = loader.close().await;
            Ok(ok)
        }
        Err(e) => {
            let _ = loader.close().await;
            Err(format!("连接测试失败: {}", e))
        }
    }
}

// ---------------------------------------------------------------------------
// LLM 集成
// ---------------------------------------------------------------------------

/// 通过 LLM 解析自然语言文本中的数据库连接信息。
#[tauri::command]
pub async fn parse_db_info(text: String, llm_config: LlmConfig) -> Result<DatabaseConfig, String> {
    let client = llm::LlmClient::new(llm_config);
    match client.parse_database_info(&text).await {
        Ok(parsed) => Ok(parsed.to_database_config()),
        Err(e) => Err(format!("LLM 解析失败: {}", e)),
    }
}

/// 测试 LLM 配置是否可用（探活）。
#[tauri::command]
pub async fn test_llm_connection(config: LlmConfig) -> Result<bool, String> {
    let client = llm::LlmClient::new(config);
    client
        .validate_config()
        .await
        .map_err(|e| format!("LLM 配置非法: {}", e))
}

// ---------------------------------------------------------------------------
// 数据加载（核心）
// ---------------------------------------------------------------------------

/// 启动批量加载任务。
///
/// 命令立即返回，真正的加载在后台 tokio 任务中执行，过程中通过 Tauri
/// 事件系统将以下事件推送到前端：
/// - `loading-progress`：[`crate::models::LoadProgress`] 单表实时进度
/// - `table-completed`：[`crate::models::TableReport`] 单表加载完成报告
/// - `loading-completed`：[`LoadReport`] 整体完成汇总报告
/// - `loading-error`：`String` 错误信息（致命错误或单表错误）
#[tauri::command]
pub async fn start_loading(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
    db_config: DatabaseConfig,
) -> Result<(), String> {
    // 防止重复启动
    {
        let mut loading = state.is_loading.lock().await;
        if *loading {
            return Err("已有加载任务在执行中".to_string());
        }
        *loading = true;
    }
    {
        let mut cancel = state.cancel_flag.lock().await;
        *cancel = false;
    }

    let last_report = Arc::clone(&state.last_report);
    let is_loading = Arc::clone(&state.is_loading);
    let cancel_flag = Arc::clone(&state.cancel_flag);
    let app_handle = app.clone();

    tokio::spawn(async move {
        let result = run_loading_task(app_handle.clone(), &path, &db_config, &cancel_flag).await;
        match result {
            Ok(report) => {
                {
                    let mut slot = last_report.lock().await;
                    *slot = Some(report.clone());
                }
                let _ = app_handle.emit("loading-completed", &report);
            }
            Err(e) => {
                error!("加载任务失败: {}", e);
                let _ = app_handle.emit("loading-error", e.to_string());
            }
        }
        let mut loading = is_loading.lock().await;
        *loading = false;
    });

    Ok(())
}

/// 实际执行批量加载的内部协程。
async fn run_loading_task(
    app: AppHandle,
    path: &str,
    db_config: &DatabaseConfig,
    cancel_flag: &Arc<Mutex<bool>>,
) -> anyhow::Result<LoadReport> {
    let dir = PathBuf::from(path);
    let pairs = parser::scan_directory(&dir).map_err(|e| anyhow::anyhow!("扫描目录失败: {}", e))?;
    info!("加载任务启动：发现 {} 组文件对", pairs.len());

    if pairs.is_empty() {
        anyhow::bail!("目录中未发现可加载的 GUT 文件对");
    }

    let loader = database::create_loader(db_config)
        .await
        .map_err(|e| anyhow::anyhow!("数据库连接失败: {}", e))?;

    let mut report_gen = ReportGenerator::new();

    for pair in &pairs {
        // 协作式取消
        if *cancel_flag.lock().await {
            warn!("用户取消，提前结束剩余表的加载");
            break;
        }

        let flg_path = PathBuf::from(&pair.flg_path);
        let dat_path = PathBuf::from(&pair.dat_path);

        // 进度通道：后端 -> emit 转发协程
        let (progress_tx, mut progress_rx) = tokio::sync::mpsc::channel(64);
        let app_for_progress = app.clone();
        let forwarder = tokio::spawn(async move {
            while let Some(progress) = progress_rx.recv().await {
                let _ = app_for_progress.emit("loading-progress", &progress);
            }
        });

        match loader::batch::load_table(loader.as_ref(), &flg_path, &dat_path, Some(progress_tx))
            .await
        {
            Ok(table_report) => {
                let _ = app.emit("table-completed", &table_report);
                report_gen.add_table_report(table_report);
            }
            Err(e) => {
                let msg = format!("表 [{}] 加载失败: {}", pair.table_name, e);
                error!("{}", msg);
                let _ = app.emit("loading-error", &msg);
            }
        }

        // 等待进度转发协程退出（progress_tx 在 load_table 内部 drop 后通道关闭）
        let _ = forwarder.await;
    }

    let _ = loader.close().await;

    Ok(report_gen.generate())
}

// ---------------------------------------------------------------------------
// 加载控制
// ---------------------------------------------------------------------------

/// 请求停止当前加载任务（协作式取消，会在当前表完成后退出循环）。
#[tauri::command]
pub async fn stop_loading(state: State<'_, AppState>) -> Result<(), String> {
    let mut cancel = state.cancel_flag.lock().await;
    *cancel = true;
    Ok(())
}

// ---------------------------------------------------------------------------
// 调试日志
// ---------------------------------------------------------------------------

/// 获取应用运行日志，供前端调试面板展示。
#[tauri::command]
pub fn get_app_logs() -> Vec<String> {
    crate::log_buffer::get_all_logs()
        .iter()
        .map(|e| format!("[{}] [{}] {}: {}", e.timestamp, e.level, e.target, e.message))
        .collect()
}

/// 清空调试日志缓冲区。
#[tauri::command]
pub fn clear_app_logs() {
    crate::log_buffer::clear_logs();
}

// ---------------------------------------------------------------------------
// 报告
// ---------------------------------------------------------------------------

/// 获取最近一次加载任务的汇总报告。
#[tauri::command]
pub async fn get_report(state: State<'_, AppState>) -> Result<Option<LoadReport>, String> {
    let report = state.last_report.lock().await;
    Ok(report.clone())
}

/// 把当前报告以 pretty JSON 写入到指定路径，返回 JSON 字符串。
#[tauri::command]
pub async fn save_report(state: State<'_, AppState>, path: String) -> Result<String, String> {
    let report = {
        let guard = state.last_report.lock().await;
        guard.clone()
    };
    match report {
        Some(r) => {
            let json = serde_json::to_string_pretty(&r).map_err(|e| e.to_string())?;
            std::fs::write(&path, &json).map_err(|e| e.to_string())?;
            Ok(json)
        }
        None => Err("没有可用的报告".to_string()),
    }
}
