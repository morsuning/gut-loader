//! 测试数据生成器
//!
//! 用法：
//!   cargo run --release --bin generate_test_data -- --rows 50000 --output ./benchmark_data/50k
//!   cargo run --release --bin generate_test_data -- --rows 500000 --output ./benchmark_data/500k
//!   cargo run --release --bin generate_test_data -- --rows 1000000 --output ./benchmark_data/1m
//!
//! 生成与 example_data/employee 严格一致的 GUT 双文件（flg + dat.gz）：
//! - ROWLENGTH=212，6 字段
//! - 数据行末尾使用 CRLF
//! - 使用 ASCII 字符填充以保证每字段精确字节数
//!
//! 表名：`benchmark_employee`，日期为 20260526。

use std::io::Write;
use std::path::PathBuf;

use flate2::write::GzEncoder;
use flate2::Compression;

const TABLE_NAME: &str = "benchmark_employee";
const DATE: &str = "20260526";
const TIME: &str = "000000";
const SEQUENCE: &str = "0000";
const ROW_LENGTH: usize = 212;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let rows = parse_arg_usize(&args, "--rows", 50_000);
    let output_dir = parse_arg_str(&args, "--output", "./benchmark_data");
    let compression_level = parse_arg_usize(&args, "--compression", 1); // 1 = fast

    if let Err(err) = generate_benchmark_data(&output_dir, rows, compression_level as u32) {
        eprintln!("生成测试数据失败: {err:?}");
        std::process::exit(1);
    }
}

fn parse_arg_str(args: &[String], key: &str, default: &str) -> String {
    for i in 0..args.len() {
        if args[i] == key {
            if let Some(v) = args.get(i + 1) {
                return v.clone();
            }
        }
        if let Some(rest) = args[i].strip_prefix(&format!("{}=", key)) {
            return rest.to_string();
        }
    }
    default.to_string()
}

fn parse_arg_usize(args: &[String], key: &str, default: usize) -> usize {
    let s = parse_arg_str(args, key, "");
    if s.is_empty() {
        default
    } else {
        s.parse::<usize>().unwrap_or(default)
    }
}

fn generate_benchmark_data(
    output_dir: &str,
    rows: usize,
    compression_level: u32,
) -> std::io::Result<()> {
    std::fs::create_dir_all(output_dir)?;
    let out_dir = PathBuf::from(output_dir);

    let stem = format!("{TABLE_NAME}.{DATE}.{TIME}.{SEQUENCE}");
    let flg_path = out_dir.join(format!("{stem}.flg"));
    let dat_path = out_dir.join(format!("{stem}.dat.gz"));

    // FLG 文件：字段定义与 employee.flg 一致（VARCHAR 长度 50/50/50/30，DECIMAL 12,2）
    let flg_content = format!(
        "{stem}.dat.gz 0 {rows} {DATE}{TIME}\n\
FILENAME={stem}.dat.gz\n\
FILESIZE=0\n\
ROWCOUNT={rows}\n\
CREATEDATETIME={DATE}{TIME}\n\
SQL=SELECT * FROM {TABLE_NAME}\n\
ROWLENGTH={ROW_LENGTH}\n\
COLUMNCOUNT=6\n\
COLUMNDECRIPTION=\n\
1$$EMP_NO$$VARCHAR(20)$$(1,20)\n\
2$$EMP_NAME$$VARCHAR(50)$$(21,70)\n\
3$$DEPARTMENT$$VARCHAR(50)$$(71,120)\n\
4$$POSITION$$VARCHAR(50)$$(121,170)\n\
5$$SALARY$$DECIMAL(12,2)$$(171,182)\n\
6$$HIRE_DATE$$VARCHAR(30)$$(183,212)\n",
    );
    std::fs::write(&flg_path, &flg_content)?;

    // DAT.GZ 文件：使用 Compression::fast 模拟实际数据压缩，避免极端可压缩性
    let compression = match compression_level {
        0 => Compression::none(),
        9 => Compression::best(),
        _ => Compression::fast(),
    };
    let file = std::fs::File::create(&dat_path)?;
    let mut encoder = GzEncoder::new(file, compression);

    // 预生成行模板以提高生成速度（仅 EMP_NO 和 SALARY 随行号变化）
    let departments = [
        "Engineering",
        "Marketing",
        "Sales",
        "HR",
        "Finance",
        "Operations",
    ];
    let positions = [
        "Manager",
        "Engineer",
        "Analyst",
        "Director",
        "Associate",
        "Lead",
    ];
    let names = [
        "Alice", "Bob", "Charlie", "Diana", "Edward", "Fiona", "George", "Helen",
    ];

    let mut buf: Vec<u8> = Vec::with_capacity(ROW_LENGTH + 2);
    for i in 0..rows {
        buf.clear();

        // 1. EMP_NO: 20 bytes  ("E" + 19 位数字)
        write_field(&mut buf, &format!("E{:019}", i + 1), 20);

        // 2. EMP_NAME: 50 bytes (ASCII)
        let name = names[i % names.len()];
        write_field(&mut buf, name, 50);

        // 3. DEPARTMENT: 50 bytes
        let dept = departments[i % departments.len()];
        write_field(&mut buf, dept, 50);

        // 4. POSITION: 50 bytes
        let pos = positions[i % positions.len()];
        write_field(&mut buf, pos, 50);

        // 5. SALARY: 12 bytes
        let salary = 5000.0 + ((i as f64 * 13.37) % 95000.0);
        write_field(&mut buf, &format!("{:>12.2}", salary), 12);

        // 6. HIRE_DATE: 30 bytes
        let year = 2020 + (i % 6);
        let month = (i % 12) + 1;
        let day = (i % 28) + 1;
        let hire = format!("{:04}-{:02}-{:02} 00:00:00", year, month, day);
        write_field(&mut buf, &hire, 30);

        debug_assert_eq!(buf.len(), ROW_LENGTH, "行字节数应为 {}", ROW_LENGTH);
        buf.extend_from_slice(b"\r\n");
        encoder.write_all(&buf)?;
    }

    encoder.finish()?;

    let dat_size = std::fs::metadata(&dat_path)?.len();
    println!("生成完毕: {} 行数据", rows);
    println!("  FLG: {}", flg_path.display());
    println!("  DAT: {} ({} 字节)", dat_path.display(), dat_size);
    Ok(())
}

/// 写入定长字段：不足右侧空格填充，超长截断。
fn write_field(buf: &mut Vec<u8>, value: &str, width: usize) {
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
