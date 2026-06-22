//! Tauri 命令定义。M1a 逐步填充。

#[tauri::command]
pub fn ping() -> &'static str {
    "pong"
}
