//! Streamable HTTP transport + 进程内启动。
//!
//! 由桌面进程在启动时 [`serve`]：绑定 127.0.0.1，路由 `/mcp`，
//! 用 Bearer token 中间件鉴权。成功后写发现文件 `~/.lmnotes/mcp.json`，
//! agent 据此接入。
//!
//! transport 选型（vs stdio）：stdio 要求 agent host 自己 spawn 独立可执行子进程，
//! 与「桌面端内嵌」冲突；且 SQLite/Tantivy 不支持跨进程并发写句柄，内嵌直接复用
//! 桌面已打开的 Arc 句柄，零锁竞争、数据始终一致。主流 agent（Claude Desktop 新版、
//! Cursor、ZCode 等）已支持 streamable HTTP transport。
//!
//! 注：写 `mcp.json` 是「配置/发现副作用」而非 vault 内笔记内容，不应走 StorageBackend
//!（后者用于沙箱化笔记）。故本模块对 ADR-0002 的 std::fs 约束做局部豁免，与
//! `llm_config.rs` 写 config.json 同一处理。

#![allow(clippy::disallowed_methods)]

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use axum::extract::State;
use axum::http::{HeaderMap, Request, StatusCode};
use axum::middleware::{self, Next};
use axum::response::Response;
use axum::Router;
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use serde::Serialize;

use crate::LmnotesMcpServer;

/// MCP 暴露的工具名清单（写发现文件 + 文档用）。
pub const TOOL_NAMES: &[&str] = &[
    "search_notes",
    "read_note",
    "list_notes",
    "ask_vault",
    "get_note_links",
];

/// 发现文件内容：agent 据此接入 MCP server。
#[derive(Debug, Serialize)]
pub struct DiscoveryFile {
    /// MCP server 端点 URL（仅 127.0.0.1）。
    pub url: String,
    /// Bearer token（请求时放 `Authorization: Bearer <token>`）。
    pub token: String,
    /// transport 类型。
    pub transport: &'static str,
    /// 暴露的工具名。
    pub tools: &'static [&'static str],
    /// vault 根目录（绝对路径）。
    pub vault_root: String,
}

/// Bearer token 校验中间件。
async fn auth_middleware(
    State(token): State<Arc<String>>,
    headers: HeaderMap,
    request: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let provided = headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .map(|s| s.to_string());
    match provided {
        Some(t) if t == *token => Ok(next.run(request).await),
        _ => Err(StatusCode::UNAUTHORIZED),
    }
}

/// 启动 MCP streamable HTTP server。
///
/// - `addr`：建议 `127.0.0.1:<port>`；端口为 0 时由 OS 分配。
/// - `token`：Bearer token；为空则视为不鉴权（仅本机 loopback，仍建议非空）。
/// - `lmnotes_home`：`~/.lmnotes` 目录，用于写 `mcp.json` 发现文件。
///
/// 返回实际绑定的 `SocketAddr`（供调用方写入发现文件）。函数会阻塞当前任务直到
/// server 退出（应在独立的 `spawn` 中调用）。
pub async fn serve(
    server: LmnotesMcpServer,
    addr: SocketAddr,
    token: String,
    lmnotes_home: PathBuf,
    vault_root: PathBuf,
) -> std::io::Result<SocketAddr> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    let bound = listener.local_addr()?;

    let token_arc = Arc::new(token.clone());

    // rmcp streamable HTTP service：每会话用同一 server 克隆（字段全 Arc，克隆廉价）
    let service: StreamableHttpService<LmnotesMcpServer, LocalSessionManager> =
        StreamableHttpService::new(
            move || Ok(server.clone()),
            LocalSessionManager::default().into(),
            StreamableHttpServerConfig::default(),
        );

    // /mcp 路由加 Bearer 鉴权
    let mcp_router =
        Router::new()
            .nest_service("/mcp", service)
            .layer(middleware::from_fn_with_state(
                token_arc.clone(),
                auth_middleware,
            ));

    // 写发现文件（尽力而为；失败仅日志，不阻断 server）
    let discovery = DiscoveryFile {
        url: format!("http://{bound}/mcp"),
        token: token.clone(),
        transport: "http",
        tools: TOOL_NAMES,
        vault_root: vault_root.to_string_lossy().into_owned(),
    };
    if let Err(e) = write_discovery_file(&lmnotes_home, &discovery) {
        eprintln!("[mcp] write discovery file failed: {e}");
    } else {
        eprintln!(
            "[mcp] serving on http://{bound}/mcp  (discovery: {})",
            lmnotes_home.join("mcp.json").display()
        );
    }

    axum::serve(listener, mcp_router)
        .with_graceful_shutdown(async {
            // 桌面进程退出即结束；此处不主动 ctrl-c（生命周期由宿主管）
            std::future::pending::<()>().await;
        })
        .await?;
    Ok(bound)
}

/// 写 `~/.lmnotes/mcp.json` 发现文件，并尽量设为仅属主可读（Unix 0600）。
fn write_discovery_file(
    lmnotes_home: &std::path::Path,
    discovery: &DiscoveryFile,
) -> std::io::Result<()> {
    let path = lmnotes_home.join("mcp.json");
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let text = serde_json::to_string_pretty(discovery).map_err(std::io::Error::other)?;
    std::fs::write(&path, text)?;
    restrict_file_permissions(&path);
    Ok(())
}

/// 尽力将文件权限限制为仅属主可读写（仅 Unix 有效；Windows 由 ACL 默认值决定）。
#[cfg(unix)]
fn restrict_file_permissions(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(meta) = std::fs::metadata(path) {
        let mut perms = meta.permissions();
        perms.set_mode(0o600);
        let _ = std::fs::set_permissions(path, perms);
    }
}

#[cfg(not(unix))]
fn restrict_file_permissions(_path: &std::path::Path) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovery_file_serializes() {
        let d = DiscoveryFile {
            url: "http://127.0.0.1:8000/mcp".into(),
            token: "t0k3n".into(),
            transport: "http",
            tools: TOOL_NAMES,
            vault_root: "/home/u/.lmnotes/default".into(),
        };
        let json = serde_json::to_string(&d).unwrap();
        assert!(json.contains("\"url\""));
        assert!(json.contains("\"search_notes\""));
        assert!(json.contains("\"transport\":\"http\""));
    }
}
