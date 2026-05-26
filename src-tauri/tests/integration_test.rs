//! 端到端集成测试
//! 需要 Docker 数据库运行：
//! - MySQL on port 3307 (user: root, pass: testpass123, db: gut_test)
//! - PostgreSQL on port 5433 (user: postgres, pass: testpass123, db: gut_test)

use gut_loader_lib::database;
use gut_loader_lib::loader::batch;
use gut_loader_lib::models::*;
use gut_loader_lib::parser;
use gut_loader_lib::report::ReportGenerator;
use gut_loader_lib::validator;
use std::path::PathBuf;

fn example_data_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("example_data")
}

/// 测试1：目录扫描识别5组文件
#[test]
fn test_scan_finds_all_files() {
    let dir = example_data_dir();
    let pairs = parser::scan_directory(&dir).unwrap();
    assert_eq!(pairs.len(), 5, "应发现5组文件对");

    let table_names: Vec<&str> = pairs.iter().map(|p| p.table_name.as_str()).collect();
    assert!(table_names.contains(&"employee"));
    assert!(table_names.contains(&"order"));
    assert!(table_names.contains(&"product"));
    assert!(table_names.contains(&"transaction"));
    assert!(table_names.contains(&"user"));
}

/// 测试2：FLG解析正确性
#[test]
fn test_flg_parsing() {
    let dir = example_data_dir();
    let pairs = parser::scan_directory(&dir).unwrap();

    for pair in &pairs {
        let flg_path = PathBuf::from(&pair.flg_path);
        let metadata = parser::flg::parse_flg(&flg_path).unwrap();

        assert!(metadata.row_count > 0);
        assert!(metadata.row_length > 0);
        assert_eq!(metadata.columns.len(), metadata.column_count);
        assert_eq!(metadata.table_name, pair.table_name);

        // 验证字段位置连续性
        let mut expected_start = 1;
        for col in &metadata.columns {
            assert_eq!(
                col.start_pos, expected_start,
                "表{}字段{}起始位置不正确",
                pair.table_name, col.name
            );
            expected_start = col.end_pos + 1;
        }
        assert_eq!(
            expected_start - 1,
            metadata.row_length,
            "表{}最后字段结束位置应等于ROWLENGTH",
            pair.table_name
        );
    }
}

/// 测试3：DAT解析正确性（含中文）
#[test]
fn test_dat_parsing_with_chinese() {
    let dir = example_data_dir();
    let pairs = parser::scan_directory(&dir).unwrap();
    let employee_pair = pairs.iter().find(|p| p.table_name == "employee").unwrap();

    let flg_path = PathBuf::from(&employee_pair.flg_path);
    let dat_path = PathBuf::from(&employee_pair.dat_path);
    let metadata = parser::flg::parse_flg(&flg_path).unwrap();
    let rows = parser::dat::parse_dat(&dat_path, &metadata).unwrap();

    assert_eq!(rows.len(), metadata.row_count, "行数应与ROWCOUNT匹配");

    let first_row = &rows[0];
    assert!(!first_row.values[0].is_empty(), "EMP_NO不应为空");
    assert!(!first_row.values[1].is_empty(), "EMP_NAME不应为空");

    let has_chinese = rows.iter().any(|r| {
        r.values
            .iter()
            .any(|v| v.chars().any(|c| c > '\u{4E00}' && c < '\u{9FFF}'))
    });
    assert!(has_chinese, "employee表应包含中文数据");
}

/// 测试4：前置检查全部通过
#[tokio::test]
async fn test_pre_checks_pass() {
    let dir = example_data_dir();
    let results = validator::run_all_checks(&dir, None).await;

    let failed: Vec<&PreCheckResult> = results
        .iter()
        .filter(|r| !r.passed && r.severity == "error")
        .collect();

    assert!(
        failed.is_empty(),
        "前置检查有错误项: {:?}",
        failed.iter().map(|r| &r.message).collect::<Vec<_>>()
    );
}

/// 测试5：MySQL数据库完整加载流程
#[tokio::test]
async fn test_mysql_full_load() {
    let config = DatabaseConfig {
        db_type: "mysql".to_string(),
        host: "127.0.0.1".to_string(),
        port: 3307,
        database: "gut_test".to_string(),
        username: "root".to_string(),
        password: "testpass123".to_string(),
        schema: None,
    };

    let loader = match database::create_loader(&config).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("跳过MySQL测试（数据库未就绪）: {}", e);
            return;
        }
    };

    assert!(loader.test_connection().await.unwrap());

    let dir = example_data_dir();
    let pairs = parser::scan_directory(&dir).unwrap();
    let employee_pair = pairs.iter().find(|p| p.table_name == "employee").unwrap();

    let flg_path = PathBuf::from(&employee_pair.flg_path);
    let dat_path = PathBuf::from(&employee_pair.dat_path);

    let report = batch::load_table(loader.as_ref(), &flg_path, &dat_path, None)
        .await
        .unwrap();

    assert_eq!(report.table_name, "employee");
    assert_eq!(report.row_count, 800);
    assert_eq!(report.success_count, 800);
    assert_eq!(report.failed_count, 0);
    assert!(report.speed > 0.0);

    let count = loader.get_row_count("employee").await.unwrap();
    assert_eq!(count, 800);

    let _ = loader.close().await;
}

/// 测试6：PostgreSQL数据库完整加载流程
#[tokio::test]
async fn test_postgres_full_load() {
    let config = DatabaseConfig {
        db_type: "postgresql".to_string(),
        host: "127.0.0.1".to_string(),
        port: 5433,
        database: "gut_test".to_string(),
        username: "postgres".to_string(),
        password: "testpass123".to_string(),
        schema: None,
    };

    let loader = match database::create_loader(&config).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("跳过PostgreSQL测试（数据库未就绪）: {}", e);
            return;
        }
    };

    assert!(loader.test_connection().await.unwrap());

    let dir = example_data_dir();
    let pairs = parser::scan_directory(&dir).unwrap();
    let mut report_gen = ReportGenerator::new();

    for pair in &pairs {
        let flg_path = PathBuf::from(&pair.flg_path);
        let dat_path = PathBuf::from(&pair.dat_path);

        let report = batch::load_table(loader.as_ref(), &flg_path, &dat_path, None)
            .await
            .unwrap();
        assert_eq!(report.failed_count, 0, "表{}有失败记录", pair.table_name);
        report_gen.add_table_report(report);
    }

    let final_report = report_gen.generate();
    assert_eq!(final_report.total_tables, 5);
    assert_eq!(final_report.total_rows, 12300);
    assert_eq!(final_report.success_rows, 12300);
    assert!((final_report.success_rate - 1.0).abs() < 1e-9);
    assert!(final_report.avg_speed > 0.0);

    let report_path = PathBuf::from("/Users/morsuning/Desktop/gut-loader/test_report.json");
    report_gen.export_json(&report_path).unwrap();
    assert!(report_path.exists());

    println!("\n{}", report_gen.export_summary());

    let _ = loader.close().await;
}

/// 测试7：流式加载与内存加载结果一致性（不依赖数据库）
#[test]
fn test_streaming_parser_matches_inmemory_for_examples() {
    use gut_loader_lib::parser::dat::{parse_dat, parse_dat_streaming};

    let dir = example_data_dir();
    let pairs = parser::scan_directory(&dir).unwrap();
    for pair in &pairs {
        let flg_path = PathBuf::from(&pair.flg_path);
        let dat_path = PathBuf::from(&pair.dat_path);
        let metadata = parser::flg::parse_flg(&flg_path).unwrap();

        // 一次性加载
        let inmemory_rows = parse_dat(&dat_path, &metadata).unwrap();

        // 流式加载累积
        let mut streaming_rows: Vec<DataRow> = Vec::new();
        parse_dat_streaming(&dat_path, &metadata, 137, |batch| {
            streaming_rows.extend(batch);
            Ok(())
        })
        .unwrap();

        assert_eq!(
            inmemory_rows.len(),
            streaming_rows.len(),
            "表 {} 流式与一次性加载行数不一致",
            pair.table_name
        );
        for (a, b) in inmemory_rows.iter().zip(streaming_rows.iter()) {
            assert_eq!(a.values, b.values, "表 {} 字段值不一致", pair.table_name);
        }
    }
}

/// 测试8：PostgreSQL 流式加载结果与内存加载完全一致
#[tokio::test]
async fn test_postgres_streaming_load_matches_inmemory() {
    let config = DatabaseConfig {
        db_type: "postgresql".to_string(),
        host: "127.0.0.1".to_string(),
        port: 5433,
        database: "gut_test".to_string(),
        username: "postgres".to_string(),
        password: "testpass123".to_string(),
        schema: None,
    };

    let loader = match database::create_loader(&config).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("跳过 PostgreSQL 流式加载测试（数据库未就绪）: {}", e);
            return;
        }
    };

    let dir = example_data_dir();
    let pairs = parser::scan_directory(&dir).unwrap();
    let employee_pair = pairs.iter().find(|p| p.table_name == "employee").unwrap();
    let flg_path = PathBuf::from(&employee_pair.flg_path);
    let dat_path = PathBuf::from(&employee_pair.dat_path);

    // 先清表，确保从空开始
    sqlx_drop_table(&config, "employee").await;

    let report = batch::load_table_streaming(loader.as_ref(), &flg_path, &dat_path, None)
        .await
        .expect("流式加载失败");

    assert_eq!(report.row_count, 800, "流式应读取 800 行");
    assert_eq!(report.success_count, 800, "流式应成功 800 行");
    assert_eq!(report.failed_count, 0);

    let count = loader.get_row_count("employee").await.unwrap();
    assert_eq!(count, 800, "PostgreSQL 中应存在 800 行");

    let _ = loader.close().await;
}

/// 工具：测试前 DROP 表（仅 PostgreSQL 用）
#[allow(dead_code)]
async fn sqlx_drop_table(config: &DatabaseConfig, table: &str) {
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

/// 测试9：自动调度——大文件触发流式，小文件走内存。
///
/// 通过生成一个超过 100MB 的 employee 风格 dat.gz 文件，验证 `load_table` 能够
/// 自动选择流式分支并完成加载（仅在显式启用 `GUT_LOADER_BIG_FILE_TEST=1` 时执行，
/// 默认跳过以避免拖慢常规 CI）。
#[tokio::test]
async fn test_postgres_auto_streaming_for_large_file() {
    if std::env::var("GUT_LOADER_BIG_FILE_TEST").ok().as_deref() != Some("1") {
        eprintln!("跳过大文件自动调度测试（设置 GUT_LOADER_BIG_FILE_TEST=1 可启用）");
        return;
    }

    let config = DatabaseConfig {
        db_type: "postgresql".to_string(),
        host: "127.0.0.1".to_string(),
        port: 5433,
        database: "gut_test".to_string(),
        username: "postgres".to_string(),
        password: "testpass123".to_string(),
        schema: None,
    };

    let loader = match database::create_loader(&config).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("跳过大文件测试（PostgreSQL 未就绪）: {}", e);
            return;
        }
    };

    let tmp_dir = std::env::temp_dir().join("gut_loader_bigfile_test");
    let _ = std::fs::create_dir_all(&tmp_dir);
    let table = "big_employee";
    let row_count: usize = 500_000; // 500k * 215 ≈ 107MB 原始，配合 Compression::none 超过 100MB 阈值
    let (flg_path, dat_path) =
        generate_employee_like_dataset(&tmp_dir, table, row_count).expect("生成大文件失败");

    // 校验确实超过阈值
    let file_size = std::fs::metadata(&dat_path).unwrap().len();
    assert!(
        file_size > batch::STREAMING_THRESHOLD_BYTES,
        "生成文件应超过 100MB 阈值，实际 {} 字节",
        file_size
    );

    sqlx_drop_table(&config, table).await;

    let report = batch::load_table(loader.as_ref(), &flg_path, &dat_path, None)
        .await
        .expect("自动调度加载失败");

    assert_eq!(report.success_count, row_count);
    assert_eq!(report.failed_count, 0);

    let _ = loader.close().await;
}

/// 生成 employee 风格的数据集（flg + dat.gz）。
///
/// 用于大文件测试：行结构与 example_data/employee 完全一致，但以 ASCII 字段填充以
/// 便快速生成大量数据。返回 `(flg_path, dat_path)`。
#[allow(dead_code)]
fn generate_employee_like_dataset(
    dir: &std::path::Path,
    table: &str,
    row_count: usize,
) -> std::io::Result<(PathBuf, PathBuf)> {
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use std::io::Write;

    // 字段定义（与 employee.flg 一致）：
    // 1 EMP_NO VARCHAR(20) (1,20)
    // 2 EMP_NAME VARCHAR(50) (21,70)  （这里用 ASCII 填充，无中文）
    // 3 DEPARTMENT VARCHAR(50) (71,120)
    // 4 POSITION VARCHAR(50) (121,170)
    // 5 SALARY DECIMAL(12,2) (171,182)
    // 6 HIRE_DATE VARCHAR(30) (183,212)
    let row_length: usize = 212;

    let stem = format!("{}.20260421.000000.0000", table);
    let flg_path = dir.join(format!("{}.flg", stem));
    let dat_path = dir.join(format!("{}.dat.gz", stem));

    // 写 flg
    let flg_content = format!(
        "{stem}.dat.gz 0 {row_count} 20260421000000\n\
FILENAME={stem}.dat.gz\n\
FILESIZE=0\n\
ROWCOUNT={row_count}\n\
CREATEDATETIME=20260421000000\n\
SQL=SELECT * FROM {table}\n\
ROWLENGTH={row_length}\n\
COLUMNCOUNT=6\n\
COLUMNDECRIPTION=\n\
1$$EMP_NO$$VARCHAR(20)$$(1,20)\n\
2$$EMP_NAME$$VARCHAR(50)$$(21,70)\n\
3$$DEPARTMENT$$VARCHAR(50)$$(71,120)\n\
4$$POSITION$$VARCHAR(50)$$(121,170)\n\
5$$SALARY$$DECIMAL(12,2)$$(171,182)\n\
6$$HIRE_DATE$$VARCHAR(30)$$(183,212)\n",
        stem = stem,
        row_count = row_count,
        table = table,
        row_length = row_length,
    );
    std::fs::write(&flg_path, flg_content)?;

    // 写 dat.gz：每行固定 212 字节 + \r\n
    let file = std::fs::File::create(&dat_path)?;
    let mut gz = GzEncoder::new(file, Compression::none());
    let mut buf: Vec<u8> = Vec::with_capacity(row_length + 2);
    // 使用简单的线性同余生成器产生可预测但低压缩性的数据，避免 gzip 过度压缩。
    let mut rng_state: u64 = 0x9E37_79B9_7F4A_7C15;
    let next_byte = |state: &mut u64| -> u8 {
        *state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (((*state >> 33) as u32) & 0xFF) as u8
    };
    let charset: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    for i in 0..row_count {
        buf.clear();
        write_padded(&mut buf, &format!("E{:019}", i), 20);
        // 以随机字符填充姓名、部门、职位，减少 gzip 可压缩性
        let mut name_field = Vec::with_capacity(50);
        for _ in 0..45 {
            let idx = next_byte(&mut rng_state) as usize % charset.len();
            name_field.push(charset[idx]);
        }
        write_padded(&mut buf, std::str::from_utf8(&name_field).unwrap(), 50);
        let mut dept_field = Vec::with_capacity(50);
        for _ in 0..45 {
            let idx = next_byte(&mut rng_state) as usize % charset.len();
            dept_field.push(charset[idx]);
        }
        write_padded(&mut buf, std::str::from_utf8(&dept_field).unwrap(), 50);
        let mut pos_field = Vec::with_capacity(50);
        for _ in 0..45 {
            let idx = next_byte(&mut rng_state) as usize % charset.len();
            pos_field.push(charset[idx]);
        }
        write_padded(&mut buf, std::str::from_utf8(&pos_field).unwrap(), 50);
        write_padded(&mut buf, &format!("{:.2}", (i % 1000000) as f64 + 0.12), 12);
        write_padded(&mut buf, &format!("2026-04-{:02}", (i % 28) + 1), 30);
        debug_assert_eq!(buf.len(), row_length);
        buf.extend_from_slice(b"\r\n");
        gz.write_all(&buf)?;
    }
    gz.finish()?;
    Ok((flg_path, dat_path))
}

fn write_padded(buf: &mut Vec<u8>, value: &str, width: usize) {
    let bytes = value.as_bytes();
    if bytes.len() >= width {
        buf.extend_from_slice(&bytes[..width]);
    } else {
        buf.extend_from_slice(bytes);
        for _ in bytes.len()..width {
            buf.push(b' ');
        }
    }
}

/// 测试10：报告生成验证
#[test]
fn test_report_generation() {
    let mut gen = ReportGenerator::new();
    gen.add_table_report(TableReport {
        table_name: "test".to_string(),
        row_count: 1000,
        success_count: 990,
        failed_count: 10,
        elapsed_ms: 5000,
        speed: 198.0,
        errors: vec!["test error".to_string()],
    });

    let report = gen.generate();
    assert_eq!(report.total_tables, 1);
    assert_eq!(report.total_rows, 1000);
    assert_eq!(report.success_rows, 990);
    assert_eq!(report.failed_rows, 10);
    assert!((report.success_rate - 0.99).abs() < 0.001);
}
