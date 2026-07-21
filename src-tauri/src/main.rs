#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // WebKitGTK DMABUF renderer fails on some Linux setups (NVIDIA, Deepin, etc.)
    // and shows a blank/white window. See https://v2.tauri.app/develop/debug/linux-graphics/
    #[cfg(target_os = "linux")]
    {
        // SAFETY: set before WebKitGTK initializes in `cab_app_lib::run()`.
        unsafe {
            std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        }
    }

    cab_app_lib::run()
}
