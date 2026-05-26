//! GUT Loader 文件解析性能基准测试
//!
//! 使用 criterion 测量：
//! 1. FLG 文件解析速度
//! 2. DAT 文件解析速度（一次性 / 流式）
//! 3. 目录扫描速度
//!
//! 数据库写入性能由 `run_benchmark` 单独运行，因 criterion 不适合 async / 副作用任务。
//!
//! 运行：
//! ```text
//! cd src-tauri && cargo bench
//! ```
//!
//! 大数据规模（50k / 500k）需先运行：
//! ```text
//! cargo run --release --bin generate_test_data -- --rows 50000 --output ../example_data/benchmark_data/50k
//! cargo run --release --bin generate_test_data -- --rows 500000 --output ../example_data/benchmark_data/500k
//! ```

use std::path::{Path, PathBuf};
use std::time::Duration;

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use gut_loader_lib::parser;

fn example_data_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("example_data")
}

fn benchmark_data_dir(scale: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("benchmark_data")
        .join(scale)
}

/// FLG 解析性能
fn bench_flg_parse(c: &mut Criterion) {
    let dir = example_data_dir();
    let mut group = c.benchmark_group("flg_parse");
    group.measurement_time(Duration::from_secs(5));

    for table in ["employee", "transaction", "user", "order", "product"] {
        let path = dir.join(format!("{table}.20260421.000000.0000.flg"));
        if !path.exists() {
            continue;
        }
        group.bench_with_input(BenchmarkId::from_parameter(table), &path, |b, p| {
            b.iter(|| {
                let meta = parser::flg::parse_flg(black_box(p)).expect("parse flg");
                black_box(meta);
            })
        });
    }
    group.finish();
}

/// DAT 解析性能（一次性内存加载）
fn bench_dat_parse_inmemory(c: &mut Criterion) {
    let mut group = c.benchmark_group("dat_parse_inmemory");
    group.measurement_time(Duration::from_secs(10));

    // example_data 中的小/中规模
    let example_dir = example_data_dir();
    for (label, stem, rows) in [
        ("employee_800", "employee.20260421.000000.0000", 800u64),
        ("transaction_5000", "transaction.20260421.000000.0000", 5000),
    ] {
        let flg = example_dir.join(format!("{stem}.flg"));
        let dat = example_dir.join(format!("{stem}.dat.gz"));
        if !flg.exists() || !dat.exists() {
            continue;
        }
        let meta = parser::flg::parse_flg(&flg).expect("parse flg");
        group.throughput(Throughput::Elements(rows));
        group.bench_with_input(
            BenchmarkId::from_parameter(label),
            &(dat, meta),
            |b, (p, m)| {
                b.iter(|| {
                    let rows = parser::dat::parse_dat(black_box(p), black_box(m)).expect("parse");
                    black_box(rows);
                })
            },
        );
    }

    // 大规模（需提前生成）
    for (scale, rows) in [("50k", 50_000u64), ("500k", 500_000u64)] {
        let dir = benchmark_data_dir(scale);
        let stem = "benchmark_employee.20260526.000000.0000";
        let flg = dir.join(format!("{stem}.flg"));
        let dat = dir.join(format!("{stem}.dat.gz"));
        if !flg.exists() || !dat.exists() {
            eprintln!("跳过 dat_parse_inmemory/{scale}（数据未生成）");
            continue;
        }
        let meta = parser::flg::parse_flg(&flg).expect("parse flg");
        group.throughput(Throughput::Elements(rows));
        let sample_size = if rows >= 500_000 { 10 } else { 20 };
        group.sample_size(sample_size);
        group.bench_with_input(
            BenchmarkId::from_parameter(scale),
            &(dat, meta),
            |b, (p, m)| {
                b.iter(|| {
                    let rows = parser::dat::parse_dat(black_box(p), black_box(m)).expect("parse");
                    black_box(rows);
                })
            },
        );
    }
    group.finish();
}

/// DAT 解析性能（流式）
fn bench_dat_parse_streaming(c: &mut Criterion) {
    let mut group = c.benchmark_group("dat_parse_streaming");
    group.measurement_time(Duration::from_secs(10));

    for (scale, rows) in [("50k", 50_000u64), ("500k", 500_000u64)] {
        let dir = benchmark_data_dir(scale);
        let stem = "benchmark_employee.20260526.000000.0000";
        let flg = dir.join(format!("{stem}.flg"));
        let dat = dir.join(format!("{stem}.dat.gz"));
        if !flg.exists() || !dat.exists() {
            eprintln!("跳过 dat_parse_streaming/{scale}（数据未生成）");
            continue;
        }
        let meta = parser::flg::parse_flg(&flg).expect("parse flg");
        group.throughput(Throughput::Elements(rows));
        let sample_size = if rows >= 500_000 { 10 } else { 20 };
        group.sample_size(sample_size);
        group.bench_with_input(
            BenchmarkId::from_parameter(scale),
            &(dat, meta),
            |b, (p, m)| {
                b.iter(|| {
                    let mut count = 0usize;
                    parser::dat::parse_dat_streaming(black_box(p), black_box(m), 1000, |batch| {
                        count += batch.len();
                        Ok(())
                    })
                    .expect("streaming");
                    black_box(count);
                })
            },
        );
    }
    group.finish();
}

/// 目录扫描速度
fn bench_scan_directory(c: &mut Criterion) {
    let dir = example_data_dir();
    if !dir.exists() {
        return;
    }
    let mut group = c.benchmark_group("scan_directory");
    group.measurement_time(Duration::from_secs(5));
    group.bench_function("example_data_5_pairs", |b| {
        b.iter(|| {
            let pairs = parser::scan_directory(black_box(&dir)).expect("scan");
            black_box(pairs);
        })
    });
    group.finish();
}

/// 在所有基准之前打印数据目录提示。
fn ensure_benchmark_dirs() {
    for scale in ["50k", "500k"] {
        let dir = benchmark_data_dir(scale);
        if !Path::new(&dir).exists() {
            eprintln!(
                "提示：未发现 {} 测试数据，部分基准将被跳过。请先运行 generate_test_data。",
                dir.display()
            );
        }
    }
}

fn bench_all(c: &mut Criterion) {
    ensure_benchmark_dirs();
    bench_flg_parse(c);
    bench_dat_parse_inmemory(c);
    bench_dat_parse_streaming(c);
    bench_scan_directory(c);
}

criterion_group! {
    name = benches;
    config = Criterion::default().warm_up_time(Duration::from_secs(2));
    targets = bench_all
}
criterion_main!(benches);
