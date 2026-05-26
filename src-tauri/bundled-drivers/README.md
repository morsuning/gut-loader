# 打包驱动说明

本目录用于存放需要随应用一起分发的数据库驱动文件。

## 达梦 DM ODBC 驱动

应用在 Windows x64、Linux x64 与 Linux arm64 版本中使用 ODBC 连接达梦数据库，运行时会自动从本目录加载驱动。macOS 版本不提供达梦数据库入口。

### 获取驱动

从达梦官网下载 DM8 数据库安装包：https://www.dameng.com/download/

安装后在安装介质的对应平台目录中找到 ODBC 驱动文件。

### 各平台所需文件

| 平台 | 文件名 | 放置目录 |
| --- | --- | --- |
| Linux x64 | libdmodbc.so | dm-odbc/linux/x64/ |
| Linux arm64 | libdmodbc.so | dm-odbc/linux/arm64/ |
| Windows x64 | dmodbc.dll | dm-odbc/windows/x64/ |

### 注意事项

- 驱动文件为达梦商业软件，不应提交到版本控制
- 构建发布版本前，确保目标平台的驱动文件已放入对应目录
- 如果驱动文件不存在，应用会尝试使用系统已安装的 DM ODBC 驱动作为回退

## Oracle

Oracle 适配器使用 oracle-rs 纯 Rust 实现（TNS 协议），无需任何额外驱动文件。
