//! 应用日志缓冲模块：捕获 tracing 日志输出，供前端调试面板查询。
//!
//! 使用环形缓冲区存储最近的日志条目，避免内存无限增长。

use std::collections::VecDeque;
use std::sync::{Arc, OnceLock, RwLock};
use tracing::Subscriber;
use tracing_subscriber::layer::Context;
use tracing_subscriber::Layer;

/// 单条日志记录
#[derive(Debug, Clone)]
pub struct LogEntry {
    pub timestamp: String,
    pub level: String,
    pub target: String,
    pub message: String,
}

/// 全局日志环形缓冲区
static LOG_BUFFER: OnceLock<Arc<RwLock<VecDeque<LogEntry>>>> = OnceLock::new();

const MAX_LOG_ENTRIES: usize = 2000;

fn buffer() -> &'static Arc<RwLock<VecDeque<LogEntry>>> {
    LOG_BUFFER.get_or_init(|| Arc::new(RwLock::new(VecDeque::with_capacity(MAX_LOG_ENTRIES))))
}

/// 追加一条日志到缓冲区
fn push_entry(entry: LogEntry) {
    if let Ok(mut buf) = buffer().write() {
        if buf.len() >= MAX_LOG_ENTRIES {
            buf.pop_front();
        }
        buf.push_back(entry);
    }
}

/// 获取缓冲区中的所有日志条目（克隆后返回）
pub fn get_all_logs() -> Vec<LogEntry> {
    buffer()
        .read()
        .map(|buf| buf.iter().cloned().collect())
        .unwrap_or_default()
}

/// 清空日志缓冲区
pub fn clear_logs() {
    if let Ok(mut buf) = buffer().write() {
        buf.clear();
    }
}

/// tracing Layer 实现：将日志事件写入环形缓冲区
pub struct BufferLayer;

impl<S: Subscriber> Layer<S> for BufferLayer {
    fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
        let metadata = event.metadata();
        let level = metadata.level();
        // 捕获 WARN 及以上级别的所有日志（TRACE<DEBUG<INFO<WARN<ERROR）
        // 仅对 DEBUG/INFO 级别过滤，保留数据库模块的 DEBUG 日志用于连接诊断
        if *level < tracing::Level::INFO
            && !metadata.target().starts_with("gut_loader_lib::database")
        {
            return;
        }

        let mut visitor = MessageVisitor(String::new());
        event.record(&mut visitor);

        let entry = LogEntry {
            timestamp: chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f").to_string(),
            level: level.to_string(),
            target: metadata.target().to_string(),
            message: visitor.0,
        };
        push_entry(entry);
    }
}

/// 简单的 visitor，只提取 message 字段
struct MessageVisitor(String);

impl tracing::field::Visit for MessageVisitor {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.0 = format!("{:?}", value);
        } else if self.0.is_empty() {
            self.0 = format!("{}={:?}", field.name(), value);
        } else {
            self.0 = format!("{} {}={:?}", self.0, field.name(), value);
        }
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "message" {
            self.0 = value.to_string();
        } else if self.0.is_empty() {
            self.0 = format!("{}={}", field.name(), value);
        } else {
            self.0 = format!("{} {}={}", self.0, field.name(), value);
        }
    }
}
