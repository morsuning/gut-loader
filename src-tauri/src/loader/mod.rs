//! 加载器模块：负责并发批量将解析后的数据写入目标数据库。

pub mod batch;

pub use batch::{load_table, load_table_inmemory, load_table_streaming, STREAMING_THRESHOLD_BYTES};
