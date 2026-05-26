//! 端到端性能基准运行器
//!
//! 覆盖：
//! - 文件解析性能（FLG / DAT 一次性、流式）
//! - 数据库写入性能（PostgreSQL 多 batch_size）
//! - 端到端解析+入库性能
//! - 内存峰值（粗略 RSS 采样）
//!
//! 用法：
//! ```text
//! cargo run --release --bin run_benchmark -- --benchmark-data ../example_data/benchmark_data \
//!     --pg-host 127.0.0.1 --pg-port 5433 --pg-user postgres --pg-pass testpass123 --pg-db gut_bench \
//!     --output ../example_data/benchmark_report.md
//! ```
//!
//! 数据库连接失败时仅跳过写入相关测试，不影响其他指标。

use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::Result;
use gut_loader_lib::database::{self};
use gut_loader_lib::loader::batch;
use gut_loader_lib::models::{DataRow, DatabaseConfig, FlgMetadata, TableReport};
use gut_loader_lib::parser;

struct Args {
    benchmark_data: PathBuf,
    pg_host: String,
    pg_port: u16,
    pg_user: String,
    pg_pass: String,
    pg_db: String,
    output: PathBuf,
    skip_db: bool,
}

fn parse_args() -> Args {
    let raw: Vec<String> = std::env::args().collect();
    let get = |key: &str, default: &str| -> String {
        for i in 0..raw.len() {
            if raw[i] == key {
                if let Some(v) = raw.get(i + 1) {
                    return v.clone();
                }
            }
            if let Some(rest) = raw[i].strip_prefix(&format!("{}=", key)) {
                return rest.to_string();
            }
        }
        default.to_string()
    };
    let skip_db = raw.iter().any(|s| s == "--skip-db");
    Args {
        benchmark_data: PathBuf::from(get("--benchmark-data", "../example_data/benchmark_data")),
        pg_host: get("--pg-host", "127.0.0.1"),
        pg_port: get("--pg-port", "5433").parse().unwrap_or(5433),
        pg_user: get("--pg-user", "postgres"),
        pg_pass: get("--pg-pass", "testpass123"),
        pg_db: get("--pg-db", "gut_bench"),
        output: PathBuf::from(get("--output", "../docs/benchmark_report.md")),
        skip_db,
    }
}

#[derive(Default)]
struct ParseResult {
    label: String,
    rows: usize,
    elapsed_ms: f64,
    rows_per_sec: f64,
}

#[derive(Default)]
struct DbResult {
    batch_size: usize,
    rows: usize,
    elapsed_ms: u128,
    rows_per_sec: f64,
}

#[derive(Default)]
struct E2EResult {
    label: String,
    rows: usize,
    elapsed_ms: u128,
    rows_per_sec: f64,
}

#[derive(Default)]
struct MemoryResult {
    label: String,
    rows: usize,
    peak_rss_mb: f64,
}

struct AllResults {
    flg_parse_ms: f64,
    dat_inmemory: Vec<ParseResult>,
    dat_streaming: Vec<ParseResult>,
    scan_us: f64,
    db_postgres: Vec<DbResult>,
    e2e: Vec<E2EResult>,
    memory: Vec<MemoryResult>,
    pg_available: bool,
    pg_skip_reason: Option<String>,
    sys_info: SysInfo,
}

struct SysInfo {
    os: String,
    cpu: String,
    rust: String,
    date: String,
}

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() -> Result<()> {
    let args = parse_args();
    let sys = collect_sys_info();
    println!("=== GUT Loader 性能基准测试 ===");
    println!("OS: {}", sys.os);
    println!("Rust: {}", sys.rust);
    println!("基准数据目录: {}", args.benchmark_data.display());

    // 1. FLG 解析
    println!("\n[1/5] FLG 解析性能...");
    let flg_parse_ms = measure_flg_parse();

    // 2. DAT 解析 (一次性 + 流式)
    println!("[2/5] DAT 解析性能（一次性 / 流式）...");
    let (dat_inmemory, dat_streaming, memory_parse) = measure_dat_parse(&args.benchmark_data);

    // 3. 目录扫描
    println!("[3/5] 目录扫描性能...");
    let scan_us = measure_scan_directory();

    // 4. 数据库写入
    println!("[4/5] 数据库写入性能（PostgreSQL）...");
    let pg_config = DatabaseConfig {
        db_type: "postgresql".to_string(),
        host: args.pg_host.clone(),
        port: args.pg_port,
        database: args.pg_db.clone(),
        username: args.pg_user.clone(),
        password: args.pg_pass.clone(),
        schema: None,
    };

    let (db_postgres, pg_available, pg_skip_reason) = if args.skip_db {
        (Vec::new(), false, Some("用户指定 --skip-db".to_string()))
    } else {
        measure_postgres_writes(&pg_config, &args.benchmark_data).await
    };

    // 5. 端到端
    println!("[5/5] 端到端性能（解析 + 入库）...");
    let (e2e, memory_e2e) = if pg_available {
        measure_e2e(&pg_config, &args.benchmark_data).await
    } else {
        (Vec::new(), Vec::new())
    };

    let mut memory = memory_parse;
    memory.extend(memory_e2e);

    let results = AllResults {
        flg_parse_ms,
        dat_inmemory,
        dat_streaming,
        scan_us,
        db_postgres,
        e2e,
        memory,
        pg_available,
        pg_skip_reason,
        sys_info: sys,
    };

    let report = render_report(&results);
    println!("\n{report}");

    if let Some(parent) = args.output.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    std::fs::write(&args.output, &report)?;
    println!("\n报告已保存到: {}", args.output.display());
    Ok(())
}

fn example_data_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("example_data")
}

fn measure_flg_parse() -> f64 {
    let dir = example_data_dir();
    let paths: Vec<PathBuf> = ["employee", "transaction", "user", "order", "product"]
        .into_iter()
        .map(|t| dir.join(format!("{t}.20260421.000000.0000.flg")))
        .filter(|p| p.exists())
        .collect();
    if paths.is_empty() {
        return 0.0;
    }
    let iters = 200;
    let start = Instant::now();
    for _ in 0..iters {
        for p in &paths {
            let m = parser::flg::parse_flg(p).expect("flg");
            std::hint::black_box(m);
        }
    }
    let total_ns = start.elapsed().as_nanos() as f64;
    total_ns / (iters as f64 * paths.len() as f64) / 1_000_000.0
}

fn measure_dat_parse(
    benchmark_root: &Path,
) -> (Vec<ParseResult>, Vec<ParseResult>, Vec<MemoryResult>) {
    let mut inmemory = Vec::new();
    let mut streaming = Vec::new();
    let mut memory = Vec::new();

    // example_data 小/中规模
    let ex = example_data_dir();
    for (label, stem) in [
        ("800 行 (employee)", "employee.20260421.000000.0000"),
        ("5,000 行 (transaction)", "transaction.20260421.000000.0000"),
    ] {
        let flg = ex.join(format!("{stem}.flg"));
        let dat = ex.join(format!("{stem}.dat.gz"));
        if !flg.exists() || !dat.exists() {
            continue;
        }
        let meta = parser::flg::parse_flg(&flg).expect("flg");
        let rows_expected = meta.row_count;
        let (elapsed_ms, _) = time_parse_dat(&dat, &meta, 20);
        inmemory.push(ParseResult {
            label: label.to_string(),
            rows: rows_expected,
            elapsed_ms,
            rows_per_sec: rows_per_sec_f(rows_expected, elapsed_ms),
        });
    }

    // benchmark_data 大/超大规模
    for (label, scale) in [
        ("50,000 行", "50k"),
        ("500,000 行", "500k"),
        ("1,000,000 行", "1m"),
    ] {
        let dir = benchmark_root.join(scale);
        let stem = "benchmark_employee.20260526.000000.0000";
        let flg = dir.join(format!("{stem}.flg"));
        let dat = dir.join(format!("{stem}.dat.gz"));
        if !flg.exists() || !dat.exists() {
            println!("  [skip] {scale}（数据未生成）");
            continue;
        }
        let meta = parser::flg::parse_flg(&flg).expect("flg");
        let rows_expected = meta.row_count;

        // 一次性 + 内存峰值
        let rss_before = current_rss_mb();
        let (elapsed_ms, peak_rss) = time_parse_dat(&dat, &meta, 3);
        inmemory.push(ParseResult {
            label: label.to_string(),
            rows: rows_expected,
            elapsed_ms,
            rows_per_sec: rows_per_sec_f(rows_expected, elapsed_ms),
        });
        memory.push(MemoryResult {
            label: format!("一次性加载 {label}"),
            rows: rows_expected,
            peak_rss_mb: (peak_rss - rss_before).max(peak_rss),
        });

        // 流式 + 内存峰值
        let rss_before2 = current_rss_mb();
        let (elapsed_ms_s, peak_rss_s) = time_parse_streaming(&dat, &meta, 1000, 3);
        streaming.push(ParseResult {
            label: label.to_string(),
            rows: rows_expected,
            elapsed_ms: elapsed_ms_s,
            rows_per_sec: rows_per_sec_f(rows_expected, elapsed_ms_s),
        });
        memory.push(MemoryResult {
            label: format!("流式加载 {label}"),
            rows: rows_expected,
            peak_rss_mb: (peak_rss_s - rss_before2).max(peak_rss_s),
        });
    }

    (inmemory, streaming, memory)
}

fn time_parse_dat(dat: &Path, meta: &FlgMetadata, iters: usize) -> (f64, f64) {
    let mut peak: f64 = 0.0;
    let start = Instant::now();
    for _ in 0..iters {
        let rows = parser::dat::parse_dat(dat, meta).expect("parse");
        let rss = current_rss_mb();
        if rss > peak {
            peak = rss;
        }
        std::hint::black_box(rows);
    }
    let avg_ns = start.elapsed().as_nanos() as f64 / iters as f64;
    let avg_ms = avg_ns / 1_000_000.0;
    (avg_ms, peak)
}

fn time_parse_streaming(
    dat: &Path,
    meta: &FlgMetadata,
    batch_size: usize,
    iters: usize,
) -> (f64, f64) {
    let mut peak: f64 = 0.0;
    let start = Instant::now();
    for _ in 0..iters {
        let mut count = 0usize;
        parser::dat::parse_dat_streaming(dat, meta, batch_size, |batch| {
            count += batch.len();
            Ok(())
        })
        .expect("streaming");
        let rss = current_rss_mb();
        if rss > peak {
            peak = rss;
        }
        std::hint::black_box(count);
    }
    let avg_ns = start.elapsed().as_nanos() as f64 / iters as f64;
    let avg_ms = avg_ns / 1_000_000.0;
    (avg_ms, peak)
}

fn measure_scan_directory() -> f64 {
    let dir = example_data_dir();
    if !dir.exists() {
        return 0.0;
    }
    let iters = 1000;
    let start = Instant::now();
    for _ in 0..iters {
        let pairs = parser::scan_directory(&dir).expect("scan");
        std::hint::black_box(pairs);
    }
    let total_ns = start.elapsed().as_nanos() as f64;
    total_ns / iters as f64 / 1000.0 // us
}

async fn measure_postgres_writes(
    config: &DatabaseConfig,
    benchmark_root: &Path,
) -> (Vec<DbResult>, bool, Option<String>) {
    let loader = match database::create_loader(config).await {
        Ok(l) => l,
        Err(e) => {
            let msg = format!("PostgreSQL 不可用: {e}");
            eprintln!("  {msg}");
            return (Vec::new(), false, Some(msg));
        }
    };
    if !loader.test_connection().await.unwrap_or(false) {
        let msg = "PostgreSQL 连接测试失败".to_string();
        eprintln!("  {msg}");
        return (Vec::new(), false, Some(msg));
    }

    let dir = benchmark_root.join("50k");
    let stem = "benchmark_employee.20260526.000000.0000";
    let flg = dir.join(format!("{stem}.flg"));
    let dat = dir.join(format!("{stem}.dat.gz"));
    if !flg.exists() || !dat.exists() {
        let msg = "benchmark_data/50k 不存在，跳过 PostgreSQL 写入测试".to_string();
        eprintln!("  {msg}");
        return (Vec::new(), true, Some(msg));
    }

    let meta = parser::flg::parse_flg(&flg).expect("flg");
    let rows = parser::dat::parse_dat(&dat, &meta).expect("dat");
    let total = rows.len();
    println!("  已加载 {total} 行用于 PostgreSQL 写入测试");

    let mut results = Vec::new();
    for batch_size in [200usize, 500, 1000, 2000, 5000] {
        let table = format!("bench_pg_{batch_size}");
        drop_table_postgres(config, &table).await;

        // 改变 meta.table_name 以便 create_table 用该名
        let mut meta_clone = meta.clone();
        meta_clone.table_name = table.clone();
        if let Err(e) = loader.create_table(&meta_clone).await {
            eprintln!("  create_table 失败 ({table}): {e}");
            continue;
        }

        let start = Instant::now();
        let mut success = 0usize;
        for chunk in rows.chunks(batch_size) {
            match loader.batch_insert(&table, &meta_clone, chunk).await {
                Ok(n) => success += n,
                Err(e) => {
                    eprintln!("  batch_insert 失败 (batch={batch_size}): {e}");
                    break;
                }
            }
        }
        let elapsed_ms = start.elapsed().as_millis();
        println!(
            "  batch_size={batch_size}: {success} 行，{elapsed_ms}ms ({:.0} 行/秒)",
            rows_per_sec(success, elapsed_ms)
        );
        results.push(DbResult {
            batch_size,
            rows: success,
            elapsed_ms,
            rows_per_sec: rows_per_sec(success, elapsed_ms),
        });

        drop_table_postgres(config, &table).await;
    }
    let _ = loader.close().await;
    (results, true, None)
}

async fn measure_e2e(
    config: &DatabaseConfig,
    benchmark_root: &Path,
) -> (Vec<E2EResult>, Vec<MemoryResult>) {
    let loader = match database::create_loader(config).await {
        Ok(l) => l,
        Err(_) => return (Vec::new(), Vec::new()),
    };

    let mut results = Vec::new();
    let mut memory = Vec::new();
    for (label, scale) in [("50,000 行", "50k"), ("500,000 行", "500k")] {
        let dir = benchmark_root.join(scale);
        let stem = "benchmark_employee.20260526.000000.0000";
        let flg = dir.join(format!("{stem}.flg"));
        let dat = dir.join(format!("{stem}.dat.gz"));
        if !flg.exists() || !dat.exists() {
            continue;
        }

        // 使用唯一表名避免与前面 batch_size 测试残留
        let table = format!("bench_e2e_{scale}");
        drop_table_postgres(config, &table).await;

        // 用临时 flg 覆盖表名（重写一个临时 flg 文件）
        let temp_flg = make_temp_flg_with_table(&flg, &table).expect("temp flg");

        let rss_before = current_rss_mb();
        let start = Instant::now();
        let report: TableReport =
            match batch::load_table(loader.as_ref(), &temp_flg, &dat, None).await {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("  端到端加载失败 ({label}): {e}");
                    continue;
                }
            };
        let elapsed_ms = start.elapsed().as_millis();
        let peak_rss = current_rss_mb().max(rss_before);

        println!(
            "  {label}: {} 行成功，{elapsed_ms}ms ({:.0} 行/秒)",
            report.success_count,
            rows_per_sec(report.success_count, elapsed_ms)
        );
        results.push(E2EResult {
            label: label.to_string(),
            rows: report.success_count,
            elapsed_ms,
            rows_per_sec: rows_per_sec(report.success_count, elapsed_ms),
        });
        memory.push(MemoryResult {
            label: format!("端到端 {label}"),
            rows: report.success_count,
            peak_rss_mb: peak_rss,
        });

        drop_table_postgres(config, &table).await;
        let _ = std::fs::remove_file(&temp_flg);
    }
    let _ = loader.close().await;
    (results, memory)
}

fn make_temp_flg_with_table(src: &Path, new_table: &str) -> std::io::Result<PathBuf> {
    let content = std::fs::read_to_string(src)?;
    // 仅替换 SQL 与文件名引用并不会改变 table_name；table_name 是从文件名解析的，
    // 因此重写为以 new_table 开头的文件名。
    let dir = src.parent().unwrap();
    let new_name = format!("{new_table}.20260526.000000.0000.flg");
    let new_path = dir.join(new_name);
    std::fs::write(&new_path, content)?;
    Ok(new_path)
}

async fn drop_table_postgres(config: &DatabaseConfig, table: &str) {
    use sqlx::postgres::PgPoolOptions;
    let url = format!(
        "postgres://{}:{}@{}:{}/{}",
        config.username, config.password, config.host, config.port, config.database
    );
    if let Ok(pool) = PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(std::time::Duration::from_secs(5))
        .connect(&url)
        .await
    {
        let sql = format!("DROP TABLE IF EXISTS \"{}\"", table);
        let _ = sqlx::query(&sql).execute(&pool).await;
        pool.close().await;
    }
}

fn rows_per_sec(rows: usize, elapsed_ms: u128) -> f64 {
    if elapsed_ms == 0 {
        0.0
    } else {
        rows as f64 / (elapsed_ms as f64 / 1000.0)
    }
}

fn rows_per_sec_f(rows: usize, elapsed_ms: f64) -> f64 {
    if elapsed_ms <= 0.0 {
        0.0
    } else {
        rows as f64 / (elapsed_ms / 1000.0)
    }
}

fn current_rss_mb() -> f64 {
    #[cfg(target_os = "macos")]
    {
        unsafe { macos_rss_mb() }
    }
    #[cfg(target_os = "linux")]
    {
        linux_rss_mb()
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        0.0
    }
}

#[cfg(target_os = "macos")]
unsafe fn macos_rss_mb() -> f64 {
    use std::mem::MaybeUninit;
    extern "C" {
        fn task_info(task: u32, flavor: u32, task_info: *mut u8, count: *mut u32) -> i32;
        fn mach_task_self() -> u32;
    }
    const MACH_TASK_BASIC_INFO: u32 = 20;
    // mach_task_basic_info_data_t: 64-bit fields，固定 11 个 u64 = 88 字节
    #[repr(C)]
    #[derive(Copy, Clone)]
    struct MachTaskBasicInfo {
        virtual_size: u64,
        resident_size: u64,
        resident_size_max: u64,
        user_time: [u32; 2],
        system_time: [u32; 2],
        policy: i32,
        suspend_count: i32,
    }
    let mut info: MaybeUninit<MachTaskBasicInfo> = MaybeUninit::uninit();
    let mut count: u32 = (std::mem::size_of::<MachTaskBasicInfo>() / 4) as u32;
    let r = task_info(
        mach_task_self(),
        MACH_TASK_BASIC_INFO,
        info.as_mut_ptr() as *mut u8,
        &mut count,
    );
    if r != 0 {
        return 0.0;
    }
    let info = info.assume_init();
    info.resident_size as f64 / 1024.0 / 1024.0
}

#[cfg(target_os = "linux")]
fn linux_rss_mb() -> f64 {
    let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("VmRSS:") {
            let kb: f64 = rest
                .trim()
                .split_whitespace()
                .next()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0.0);
            return kb / 1024.0;
        }
    }
    0.0
}

fn collect_sys_info() -> SysInfo {
    let os = format!("{} {}", std::env::consts::OS, std::env::consts::ARCH);
    let cpu = read_cpu_model().unwrap_or_else(|| "未知".to_string());
    let rust = rustc_version_runtime();
    let date = chrono::Local::now().format("%Y-%m-%d").to_string();
    SysInfo {
        os: format!("{os} ({cpu})"),
        cpu,
        rust,
        date,
    }
}

fn read_cpu_model() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        let out = std::process::Command::new("sysctl")
            .args(["-n", "machdep.cpu.brand_string"])
            .output()
            .ok()?;
        let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if s.is_empty() {
            None
        } else {
            Some(s)
        }
    }
    #[cfg(target_os = "linux")]
    {
        let content = std::fs::read_to_string("/proc/cpuinfo").ok()?;
        for line in content.lines() {
            if let Some(rest) = line.split_once(':').map(|p| p.1) {
                if line.starts_with("model name") {
                    return Some(rest.trim().to_string());
                }
            }
        }
        None
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        None
    }
}

fn rustc_version_runtime() -> String {
    std::process::Command::new("rustc")
        .arg("--version")
        .output()
        .ok()
        .and_then(|out| {
            let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if s.is_empty() {
                None
            } else {
                Some(s)
            }
        })
        .unwrap_or_else(|| "unknown".to_string())
}

fn render_report(r: &AllResults) -> String {
    use std::fmt::Write as _;
    let mut s = String::new();

    writeln!(s, "# GUT Loader 性能基准测试报告").ok();
    writeln!(s).ok();
    writeln!(s, "```").ok();
    writeln!(
        s,
        "═══════════════════════════════════════════════════════════"
    )
    .ok();
    writeln!(s, "               GUT Loader 性能基准测试报告").ok();
    writeln!(
        s,
        "═══════════════════════════════════════════════════════════"
    )
    .ok();
    writeln!(s).ok();
    writeln!(s, "测试环境:").ok();
    writeln!(s, "  OS:    {}", r.sys_info.os).ok();
    writeln!(s, "  CPU:   {}", r.sys_info.cpu).ok();
    writeln!(s, "  Rust:  {}", r.sys_info.rust).ok();
    writeln!(s, "  日期:  {}", r.sys_info.date).ok();
    writeln!(s).ok();
    writeln!(
        s,
        "────────────────────────────────────────────────────────────"
    )
    .ok();

    // 1. 文件解析
    writeln!(s).ok();
    writeln!(s, "1. 文件解析性能").ok();
    writeln!(s).ok();
    writeln!(s, "   FLG 解析:").ok();
    writeln!(s, "     单文件平均耗时: {:.4}ms", r.flg_parse_ms).ok();
    writeln!(s).ok();
    writeln!(s, "   DAT 解析 (一次性加载):").ok();
    for it in &r.dat_inmemory {
        writeln!(
            s,
            "     {:<26} {:>6.1}ms  ({:>10} 行/秒)",
            it.label,
            it.elapsed_ms,
            format_thousands(it.rows_per_sec as usize)
        )
        .ok();
    }
    writeln!(s).ok();
    writeln!(s, "   DAT 解析 (流式模式):").ok();
    if r.dat_streaming.is_empty() {
        writeln!(s, "     （未生成 50k/500k 数据，跳过）").ok();
    }
    for it in &r.dat_streaming {
        writeln!(
            s,
            "     {:<26} {:>6.1}ms  ({:>10} 行/秒)",
            it.label,
            it.elapsed_ms,
            format_thousands(it.rows_per_sec as usize)
        )
        .ok();
    }
    writeln!(s).ok();
    writeln!(s, "   目录扫描:").ok();
    writeln!(s, "     example_data (5 组): {:.2}µs/次", r.scan_us).ok();

    // 2. 数据库写入
    writeln!(s).ok();
    writeln!(s, "2. 数据库写入性能 (PostgreSQL)").ok();
    if !r.pg_available {
        writeln!(
            s,
            "   （跳过：{}）",
            r.pg_skip_reason.as_deref().unwrap_or("PostgreSQL 不可用")
        )
        .ok();
    } else if r.db_postgres.is_empty() {
        writeln!(
            s,
            "   （PostgreSQL 已连接，但 benchmark_data/50k 不存在，跳过）"
        )
        .ok();
    } else {
        for it in &r.db_postgres {
            writeln!(
                s,
                "   batch_size={:>4}:  {} 行 {:>6}ms  ({:>10} 行/秒)",
                it.batch_size,
                format_thousands(it.rows),
                it.elapsed_ms,
                format_thousands(it.rows_per_sec as usize)
            )
            .ok();
        }
        if let Some(best) = r
            .db_postgres
            .iter()
            .max_by(|a, b| a.rows_per_sec.partial_cmp(&b.rows_per_sec).unwrap())
        {
            writeln!(s).ok();
            writeln!(
                s,
                "   最佳配置: batch_size={} ({:.0} 行/秒)",
                best.batch_size, best.rows_per_sec
            )
            .ok();
        }
    }

    // 3. 端到端
    writeln!(s).ok();
    writeln!(s, "3. 端到端性能 (解析 + 入库)").ok();
    if r.e2e.is_empty() {
        writeln!(s, "   （跳过：未运行端到端测试）").ok();
    } else {
        for it in &r.e2e {
            writeln!(
                s,
                "     {:<14} {:>6}ms  ({:>10} 行/秒)",
                it.label,
                it.elapsed_ms,
                format_thousands(it.rows_per_sec as usize)
            )
            .ok();
        }
    }

    // 4. 内存
    writeln!(s).ok();
    writeln!(s, "4. 内存特征").ok();
    if r.memory.is_empty() {
        writeln!(s, "   （未采集内存数据）").ok();
    } else {
        for it in &r.memory {
            writeln!(s, "   {:<28} 峰值 RSS ~ {:.1} MB", it.label, it.peak_rss_mb).ok();
        }
    }

    writeln!(s).ok();
    writeln!(
        s,
        "═══════════════════════════════════════════════════════════"
    )
    .ok();
    writeln!(s, "```").ok();

    // markdown 表格补充
    writeln!(s).ok();
    writeln!(s, "## 数据摘要").ok();
    writeln!(s).ok();
    writeln!(s, "### DAT 解析（一次性）").ok();
    writeln!(s).ok();
    writeln!(s, "| 规模 | 行数 | 耗时(ms) | 吞吐(行/秒) |").ok();
    writeln!(s, "|------|------|----------|-------------|").ok();
    for it in &r.dat_inmemory {
        writeln!(
            s,
            "| {} | {} | {:.1} | {} |",
            it.label,
            format_thousands(it.rows),
            it.elapsed_ms,
            format_thousands(it.rows_per_sec as usize)
        )
        .ok();
    }
    writeln!(s).ok();
    if !r.dat_streaming.is_empty() {
        writeln!(s, "### DAT 解析（流式）").ok();
        writeln!(s).ok();
        writeln!(s, "| 规模 | 行数 | 耗时(ms) | 吞吐(行/秒) |").ok();
        writeln!(s, "|------|------|----------|-------------|").ok();
        for it in &r.dat_streaming {
            writeln!(
                s,
                "| {} | {} | {:.1} | {} |",
                it.label,
                format_thousands(it.rows),
                it.elapsed_ms,
                format_thousands(it.rows_per_sec as usize)
            )
            .ok();
        }
        writeln!(s).ok();
    }
    if !r.db_postgres.is_empty() {
        writeln!(s, "### PostgreSQL 批量写入").ok();
        writeln!(s).ok();
        writeln!(s, "| batch_size | 行数 | 耗时(ms) | 吞吐(行/秒) |").ok();
        writeln!(s, "|------------|------|----------|-------------|").ok();
        for it in &r.db_postgres {
            writeln!(
                s,
                "| {} | {} | {} | {} |",
                it.batch_size,
                format_thousands(it.rows),
                it.elapsed_ms,
                format_thousands(it.rows_per_sec as usize)
            )
            .ok();
        }
        writeln!(s).ok();
    }
    if !r.e2e.is_empty() {
        writeln!(s, "### 端到端").ok();
        writeln!(s).ok();
        writeln!(s, "| 规模 | 行数 | 耗时(ms) | 吞吐(行/秒) |").ok();
        writeln!(s, "|------|------|----------|-------------|").ok();
        for it in &r.e2e {
            writeln!(
                s,
                "| {} | {} | {} | {} |",
                it.label,
                format_thousands(it.rows),
                it.elapsed_ms,
                format_thousands(it.rows_per_sec as usize)
            )
            .ok();
        }
        writeln!(s).ok();
    }
    if !r.memory.is_empty() {
        writeln!(s, "### 内存峰值").ok();
        writeln!(s).ok();
        writeln!(s, "| 场景 | 行数 | 峰值 RSS (MB) |").ok();
        writeln!(s, "|------|------|---------------|").ok();
        for it in &r.memory {
            writeln!(
                s,
                "| {} | {} | {:.1} |",
                it.label,
                format_thousands(it.rows),
                it.peak_rss_mb
            )
            .ok();
        }
    }

    s
}

fn format_thousands(n: usize) -> String {
    let s = n.to_string();
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    let len = bytes.len();
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 && (len - i) % 3 == 0 {
            out.push(',');
        }
        out.push(*b as char);
    }
    out
}

// 抑制未使用警告（DataRow 在签名中可能被推断为未使用）
#[allow(dead_code)]
fn _force_use(_: &DataRow) {}
