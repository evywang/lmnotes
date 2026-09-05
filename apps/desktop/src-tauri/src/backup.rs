//! 自动备份（v0.9「搜索与常驻」）：定时把 vault 打 zip 快照到本地目录，
//! 保留最近 N 份。zip 逻辑与手动导出（export_vault_zip）共用 `zip_vault_to`。
//!
//! 数据安全三件套的最后一块：派生数据可重建（删 .lmnotes/）、手动导出
//! （v0.5）之后，本模块补上「不依赖用户记忆的定时副本」。
//!
//! 文件 IO 属 Tauri 壳层职责（ADR-0002 豁免区，同 commands.rs）。

#![allow(clippy::disallowed_methods)]

use std::path::{Path, PathBuf};

/// 备份文件名：`lmnotes-<vault 名>-<YYYYMMDD-HHMMSS>.zip`（字典序即时间序）。
pub fn backup_file_name(vault_name: &str, ts: chrono::DateTime<chrono::Utc>) -> String {
    format!("lmnotes-{vault_name}-{}.zip", ts.format("%Y%m%d-%H%M%S"))
}

/// 默认备份目录：`~/.lmnotes/backups/<vault 名>`。
pub fn default_dest_dir(vault_root: &Path) -> PathBuf {
    let vault_name = vault_root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "default".into());
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".lmnotes")
        .join("backups")
        .join(vault_name)
}

/// 修剪：目录中匹配 `lmnotes-<vault>-*.zip` 的文件按名降序（新→旧）保留
/// keep 份，其余删除。返回被删除的文件名（供日志/测试）。
pub fn prune_backups(dir: &Path, vault_name: &str, keep: usize) -> Vec<String> {
    let prefix = format!("lmnotes-{vault_name}-");
    let mut names: Vec<String> = match std::fs::read_dir(dir) {
        Ok(rd) => rd
            .flatten()
            .filter_map(|e| {
                let n = e.file_name().to_string_lossy().into_owned();
                (n.starts_with(&prefix) && n.ends_with(".zip")).then_some(n)
            })
            .collect(),
        Err(_) => return Vec::new(),
    };
    names.sort();
    names.reverse();
    let mut removed = Vec::new();
    for name in names.iter().skip(keep) {
        if std::fs::remove_file(dir.join(name)).is_ok() {
            removed.push(name.clone());
        }
    }
    removed
}

/// 递归收集（跳过 `.lmnotes/` 派生数据）——从 export_vault_zip 抽出共用。
pub fn collect_files(dir: &Path, out: &mut Vec<PathBuf>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                if p.file_name().map(|n| n == ".lmnotes").unwrap_or(false) {
                    continue;
                }
                collect_files(&p, out);
            } else {
                out.push(p);
            }
        }
    }
}

/// 把 root 打 zip 到 dest（阻塞，调用方自行 spawn_blocking）。
/// progress 每约 25 个文件回调 (done, total)。返回写入文件数。
pub fn zip_vault_to(
    root: &Path,
    dest: &Path,
    progress: Option<&(dyn Fn(u64, u64) + Send + Sync)>,
) -> Result<u64, String> {
    let mut files = Vec::new();
    collect_files(root, &mut files);
    files.sort();
    if let Some(pp) = dest.parent() {
        std::fs::create_dir_all(pp).map_err(|e| e.to_string())?;
    }
    let file = std::fs::File::create(dest).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    let total = files.len() as u64;
    for (i, abs) in files.iter().enumerate() {
        let rel = abs
            .strip_prefix(root)
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        zip.start_file(&rel, opts).map_err(|e| e.to_string())?;
        let mut f = std::fs::File::open(abs).map_err(|e| e.to_string())?;
        std::io::copy(&mut f, &mut zip).map_err(|e| e.to_string())?;
        if let Some(cb) = progress {
            if i % 25 == 0 {
                cb(i as u64, total);
            }
        }
    }
    zip.finish().map_err(|e| e.to_string())?;
    Ok(total)
}

/// 执行一次备份：zip 到 dest_dir（时间戳命名）+ 按 keep 修剪。
/// 返回 (备份文件路径, 写入文件数, 被修剪文件)。
pub fn run_backup_once(
    vault_root: &Path,
    dest_dir: &Path,
    keep: usize,
) -> Result<(PathBuf, u64, Vec<String>), String> {
    let vault_name = vault_root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "default".into());
    let name = backup_file_name(&vault_name, chrono::Utc::now());
    let dest = dest_dir.join(&name);
    let count = zip_vault_to(vault_root, &dest, None)?;
    let removed = prune_backups(dest_dir, &vault_name, keep);
    Ok((dest, count, removed))
}

/// 后台定时任务（lib.rs setup 调用）：enabled 时启动。首跑延迟 2 分钟
/// 避开启动索引高峰；此后每 interval_hours 一轮。配置读取一次性（改配置
/// 重启生效，与热键同语义）。
pub fn spawn_backup_task(app: tauri::AppHandle) {
    use crate::llm_config::Config;
    let cfg = Config::load_or_default();
    if !cfg.backup.enabled {
        return;
    }
    let interval = std::time::Duration::from_secs(cfg.backup.interval_hours.max(1) * 3600);
    let keep = cfg.backup.keep.max(1);
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(120)).await;
        loop {
            let root = crate::llm_config::current_vault();
            let dest = cfg
                .backup
                .dest_dir
                .as_ref()
                .map(PathBuf::from)
                .unwrap_or_else(|| default_dest_dir(&root));
            match tokio::task::spawn_blocking(move || run_backup_once(&root, &dest, keep)).await {
                Ok(Ok((path, n, removed))) => eprintln!(
                    "[backup] {} files -> {} (pruned {})",
                    n,
                    path.display(),
                    removed.len()
                ),
                Ok(Err(e)) => eprintln!("[backup] failed: {e}"),
                Err(e) => eprintln!("[backup] join error: {e}"),
            }
            let _ = &app;
            tokio::time::sleep(interval).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backup_file_name_is_sortable_and_namespaced() {
        let ts = chrono::Utc::now();
        let a = backup_file_name("default", ts);
        assert!(a.starts_with("lmnotes-default-"), "{a}");
        assert!(a.ends_with(".zip"));
        assert!(a.contains(&ts.format("%Y%m%d").to_string()));
    }

    #[test]
    fn prune_keeps_newest_n() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        for ts in ["20260101-000000", "20260102-000000", "20260103-000000"] {
            std::fs::write(d.join(format!("lmnotes-default-{ts}.zip")), b"z").unwrap();
        }
        // 不相关文件不动
        std::fs::write(d.join("other.txt"), b"x").unwrap();
        let removed = prune_backups(d, "default", 2);
        assert_eq!(removed, vec!["lmnotes-default-20260101-000000.zip"]);
        let mut left: Vec<String> = std::fs::read_dir(d)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".zip"))
            .collect();
        left.sort();
        assert_eq!(
            left,
            vec![
                "lmnotes-default-20260102-000000.zip",
                "lmnotes-default-20260103-000000.zip",
            ]
        );
        assert!(d.join("other.txt").exists());
    }

    #[test]
    fn zip_roundtrip_and_excludes_derived() {
        let src = tempfile::tempdir().unwrap();
        let s = src.path();
        std::fs::create_dir_all(s.join("notes")).unwrap();
        std::fs::write(s.join("notes/a.md"), b"hello").unwrap();
        std::fs::create_dir_all(s.join(".lmnotes")).unwrap();
        std::fs::write(s.join(".lmnotes/index.sqlite"), b"derived").unwrap();
        let out = tempfile::tempdir().unwrap();
        let dest = out.path().join("b.zip");
        let n = zip_vault_to(s, &dest, None).unwrap();
        assert_eq!(n, 1);
        let f = std::fs::File::open(&dest).unwrap();
        let z = zip::ZipArchive::new(f).unwrap();
        assert_eq!(z.len(), 1, "只应有 1 个文件（.lmnotes 排除）");
        let mut names = z.file_names().collect::<Vec<_>>();
        names.sort();
        assert_eq!(names, vec!["notes/a.md"]);
    }
}
