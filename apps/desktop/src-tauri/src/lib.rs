//! LMNotes 桌面应用（Tauri 2）IPC 壳。命令在 commands.rs，M1a 逐步填充。

mod commands;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![commands::ping])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
