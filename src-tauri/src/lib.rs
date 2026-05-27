pub mod commands;
pub mod database;
pub mod llm;
pub mod loader;
pub mod log_buffer;
pub mod models;
pub mod parser;
pub mod report;
pub mod validator;

use commands::AppState;
use tracing_subscriber::prelude::*;

const WEBVIEW2_DISABLE_GPU_ARGS: &str =
    "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection --disable-gpu";

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut context = tauri::generate_context!();
    configure_windows_webview_args(context.config_mut());

    // 初始化 tracing 日志系统：同时输出到 stderr 和内存缓冲区（供调试面板查询）
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().with_target(true))
        .with(log_buffer::BufferLayer)
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_fs::init())
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            commands::scan_directory,
            commands::run_pre_checks,
            commands::test_connection,
            commands::start_loading,
            commands::stop_loading,
            commands::parse_db_info,
            commands::test_llm_connection,
            commands::get_report,
            commands::save_report,
            commands::get_app_logs,
            commands::clear_app_logs,
        ])
        .setup(|_app| {
            #[cfg(debug_assertions)]
            {
                use tauri::Manager;
                let window = _app.get_webview_window("main").unwrap();
                window.open_devtools();
            }

            // Windows: 无边框窗口恢复原生阴影
            #[cfg(target_os = "windows")]
            {
                use tauri::Manager;
                if let Some(window) = _app.get_webview_window("main") {
                    enable_windows_shadow(&window);
                }
            }

            Ok(())
        })
        .run(context)
        .expect("error while running tauri application");
}

fn configure_windows_webview_args(config: &mut tauri::Config) {
    if should_disable_webview_gpu() {
        for window in &mut config.app.windows {
            // 只在确认当前显示路径是软件/虚拟渲染时禁用 GPU，避免真实显卡机器被降级。
            window.additional_browser_args = Some(WEBVIEW2_DISABLE_GPU_ARGS.to_string());
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn should_disable_webview_gpu() -> bool {
    false
}

#[cfg(target_os = "windows")]
fn should_disable_webview_gpu() -> bool {
    windows_display_adapters()
        .map(|adapters| {
            // 探测不到适配器时保持默认硬件加速路径，优先保证真实 GPU 环境的显示效果。
            !adapters.is_empty()
                && adapters.iter().all(|adapter| {
                    is_virtual_or_software_display_adapter(&adapter.name, adapter.state_flags)
                })
        })
        .unwrap_or(false)
}

#[cfg(target_os = "windows")]
struct DisplayAdapter {
    name: String,
    state_flags: u32,
}

#[cfg(target_os = "windows")]
fn windows_display_adapters() -> Option<Vec<DisplayAdapter>> {
    use std::{mem, ptr};
    use windows_sys::Win32::Graphics::Gdi::{
        EnumDisplayDevicesW, DISPLAY_DEVICEW, DISPLAY_DEVICE_ATTACHED_TO_DESKTOP,
    };

    let mut adapters = Vec::new();
    let mut index = 0;

    loop {
        let mut device: DISPLAY_DEVICEW = unsafe { mem::zeroed() };
        device.cb = mem::size_of::<DISPLAY_DEVICEW>() as u32;

        let ok = unsafe { EnumDisplayDevicesW(ptr::null(), index, &mut device, 0) };
        if ok == 0 {
            break;
        }

        if device.StateFlags & DISPLAY_DEVICE_ATTACHED_TO_DESKTOP != 0 {
            adapters.push(DisplayAdapter {
                name: wide_array_to_string(&device.DeviceString),
                state_flags: device.StateFlags,
            });
        }

        index += 1;
    }

    Some(adapters)
}

#[cfg(target_os = "windows")]
fn wide_array_to_string(chars: &[u16]) -> String {
    let len = chars.iter().position(|&ch| ch == 0).unwrap_or(chars.len());
    String::from_utf16_lossy(&chars[..len])
}

/// DWM 扩展边框结构体（windows_sys 未提供，手动定义）
#[cfg(target_os = "windows")]
#[repr(C)]
struct Margins {
    cx_left_width: i32,
    cx_right_width: i32,
    cy_top_height: i32,
    cy_bottom_height: i32,
}

/// Windows: 通过 DWM API 为无边框窗口启用原生阴影效果
#[cfg(target_os = "windows")]
fn enable_windows_shadow(window: &tauri::WebviewWindow) {
    use windows_sys::Win32::Graphics::Dwm::DwmExtendFrameIntoClientArea;

    let hwnd = window.hwnd().unwrap().0;

    // 将边框延伸到客户区，值 -1 表示扩展到所有边缘
    let margins = Margins {
        cx_left_width: -1,
        cx_right_width: -1,
        cy_top_height: -1,
        cy_bottom_height: -1,
    };

    unsafe {
        DwmExtendFrameIntoClientArea(hwnd, &margins as *const _ as *const _);
    }
}

#[cfg(target_os = "windows")]
fn is_virtual_or_software_display_adapter(name: &str, state_flags: u32) -> bool {
    use windows_sys::Win32::Graphics::Gdi::{
        DISPLAY_DEVICE_MIRRORING_DRIVER, DISPLAY_DEVICE_RDPUDD, DISPLAY_DEVICE_REMOTE,
    };

    if state_flags
        & (DISPLAY_DEVICE_REMOTE | DISPLAY_DEVICE_MIRRORING_DRIVER | DISPLAY_DEVICE_RDPUDD)
        != 0
    {
        return true;
    }

    let normalized = name.to_ascii_lowercase();
    [
        "microsoft basic display",
        "microsoft basic render",
        "microsoft remote display",
        "virtual display",
        "remote display",
        "rdp",
        "vmware",
        "svga",
        "virtualbox",
        "parallels",
        "hyper-v",
        "qxl",
        "virtio",
    ]
    .iter()
    .any(|needle| normalized.contains(needle))
}
