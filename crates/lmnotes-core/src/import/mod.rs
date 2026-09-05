//! 外部 Markdown 库导入（FR-STORE-06，v0.8）：Obsidian/Foam/纯 md 目录 → OKF。
//!
//! 本模块只做**纯逻辑**（可测）：wikilink → OKF 链接转换、frontmatter
//! best-effort 映射、目标路径规划（去重/清洗）。IO（目录扫描、落盘、
//! sha256 归档）由调用方编排（壳层 std::fs 豁免区 / FsBackend）。
//!
//! 两阶段内存内规划：第一遍建 解析映射（精确 rel > 唯一 basename > 唯一 title）
//! 与目标路径；第二遍逐文件转换 frontmatter + 重写正文链接。

use std::collections::HashMap;

/// 源 md 文件（调用方已读入文本）。rel 为相对源根的 posix 风格路径。
#[derive(Debug, Clone)]
pub struct SourceMd {
    pub rel: String,
    pub text: String,
}

/// 资源映射：源 rel → vault 内目标路径（带前导 `/`，如 `/assets/img/ab/x.png`）。
#[derive(Debug, Clone)]
pub struct AssetDest {
    pub src_rel: String,
    pub dest: String,
}

#[derive(Debug, Clone)]
pub struct ImportOptions {
    /// md 笔记落位根（如 `notes/import-20260905`）。
    pub dest_root: String,
}

impl Default for ImportOptions {
    fn default() -> Self {
        Self {
            dest_root: "notes/import".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedNote {
    pub dest: String,
    pub content: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WikiStats {
    pub resolved: usize,
    pub unresolved: usize,
}

#[derive(Debug, Clone, Default)]
pub struct ImportPlan {
    pub notes: Vec<PlannedNote>,
    pub stats: WikiStats,
    pub warnings: Vec<String>,
}

/// 解析结果：目标 vault 路径 + 是否资源（资源用 `![]()` 嵌入语义）。
type ResolveOut = (String, bool);

/// wikilink → OKF markdown 链接。
///
/// 支持形态：`[[Note]]`、`[[folder/Note]]`、`[[Note|别名]]`、`[[Note#小节]]`、
/// `![[img.png]]`（嵌入）。未解析的目标**原样保留**并计入 stats（不丢内容）。
///
/// 链接目标风格：笔记路径一律 `<>` 包裹（文件名可能含空格，CommonMark 合法）；
/// 资源路径为 sha256 产物（无空格）不包裹。资源 + 嵌入 → `![]()`；笔记嵌入
/// 降级为普通链接（OKF 无嵌入语法）。
pub fn convert_wikilinks(
    body: &str,
    resolve: &dyn Fn(&str) -> Option<ResolveOut>,
    stats: &mut WikiStats,
) -> String {
    let mut out = String::with_capacity(body.len());
    let mut rest = body;
    while let Some(pos) = rest.find("[[") {
        out.push_str(&rest[..pos]);
        let after = &rest[pos + 2..];
        match after.find("]]") {
            Some(end) => {
                let inner = &after[..end];
                // 嵌入标记：紧邻 "[[" 前的 "!"（已被 push 进 out，弹出判定）
                let embed = if out.ends_with('!') {
                    out.pop();
                    true
                } else {
                    false
                };
                match render_wikilink(inner, embed, resolve) {
                    Some(link) => {
                        stats.resolved += 1;
                        out.push_str(&link);
                    }
                    None => {
                        stats.unresolved += 1;
                        if embed {
                            out.push('!');
                        }
                        out.push_str("[[");
                        out.push_str(inner);
                        out.push_str("]]");
                    }
                }
                rest = &after[end + 2..];
            }
            None => {
                out.push_str("[[");
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// 转换单个 wikilink 内部文本；不可解析（空目标/未命中）返回 None。
fn render_wikilink(
    inner: &str,
    embed: bool,
    resolve: &dyn Fn(&str) -> Option<ResolveOut>,
) -> Option<String> {
    let (target_part, alias) = match inner.split_once('|') {
        Some((t, a)) => (t.trim(), a.trim()),
        None => (inner.trim(), ""),
    };
    let (path_target, _heading) = match target_part.split_once('#') {
        Some((p, h)) => (p.trim(), h),
        None => (target_part, ""),
    };
    if path_target.is_empty() {
        return None;
    }
    let (dest, is_asset) = resolve(path_target)?;
    // 显示文本：别名优先，否则目标 basename（去 .md）
    let text = if !alias.is_empty() {
        alias.to_string()
    } else {
        file_stem_of(path_target)
    };
    Some(if embed && is_asset {
        format!("![{alias}]({dest})")
    } else {
        format!("[{text}](<{dest}>)")
    })
}

/// frontmatter best-effort 映射：无则生成；`tags`/`aliases` 字符串→数组；
/// 补 `id`/`title`（文件名回退）/`created`；已有 `type` 保留；未知键原样保留。
/// 返回完整文件内容（frontmatter + 原 body）。
pub fn map_frontmatter(raw: &str, file_stem: &str, id: &str, now: &str) -> String {
    let (existing, body) = split_existing_frontmatter(raw);
    let mut out = serde_yaml::Mapping::new();
    let src = existing.unwrap_or_default();
    let get = |key: &str| src.get(key).cloned();
    out.insert(
        serde_yaml::Value::String("type".into()),
        get("type").unwrap_or(serde_yaml::Value::String("note".into())),
    );
    out.insert(
        serde_yaml::Value::String("id".into()),
        get("id").unwrap_or(serde_yaml::Value::String(id.into())),
    );
    out.insert(
        serde_yaml::Value::String("title".into()),
        get("title").unwrap_or(serde_yaml::Value::String(file_stem.into())),
    );
    if let Some(tags) = get("tags") {
        let coerced = coerce_string_list(&tags);
        if !coerced.is_empty() {
            out.insert(
                serde_yaml::Value::String("tags".into()),
                serde_yaml::Value::Sequence(
                    coerced.into_iter().map(serde_yaml::Value::String).collect(),
                ),
            );
        }
    }
    if let Some(aliases) = get("aliases") {
        let coerced = coerce_string_list(&aliases);
        if !coerced.is_empty() {
            out.insert(
                serde_yaml::Value::String("aliases".into()),
                serde_yaml::Value::Sequence(
                    coerced.into_iter().map(serde_yaml::Value::String).collect(),
                ),
            );
        }
    }
    out.insert(
        serde_yaml::Value::String("created".into()),
        serde_yaml::Value::String(now.into()),
    );
    // 未知键原样保留（OKF §9 消费者须容忍；保持用户数据不丢）
    for (k, v) in src {
        let key_str = k.as_str().unwrap_or("");
        if !matches!(
            key_str,
            "type" | "id" | "title" | "tags" | "aliases" | "created"
        ) {
            out.insert(k, v);
        }
    }
    let yaml = serde_yaml::to_string(&serde_yaml::Value::Mapping(out))
        .unwrap_or_else(|_| "type: note\n".into());
    format!("---\n{yaml}---\n\n{}", body.trim_start_matches('\n'))
}

/// 拆出（已有 frontmatter Mapping, body）；无/坏 frontmatter 返回 (None, 原文)。
fn split_existing_frontmatter(raw: &str) -> (Option<serde_yaml::Mapping>, String) {
    let trimmed = raw.trim_start();
    if !trimmed.starts_with("---") {
        return (None, raw.to_string());
    }
    let after_first = &trimmed[3..];
    let rest = after_first.strip_prefix('\n').unwrap_or(after_first);
    match rest.find("\n---") {
        Some(end) => {
            let yaml_str = &rest[..end];
            let body = &rest[end + 4..];
            match serde_yaml::from_str::<serde_yaml::Value>(yaml_str) {
                Ok(serde_yaml::Value::Mapping(m)) => (Some(m), body.to_string()),
                _ => (None, raw.to_string()),
            }
        }
        None => (None, raw.to_string()),
    }
}

/// 标量字符串 "a, b" / 序列 → Vec<String>；其它（含空）→ 空表。
fn coerce_string_list(v: &serde_yaml::Value) -> Vec<String> {
    match v {
        serde_yaml::Value::String(s) => s
            .split(',')
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty())
            .collect(),
        serde_yaml::Value::Sequence(seq) => seq
            .iter()
            .filter_map(|item| match item {
                serde_yaml::Value::String(s) => Some(s.clone()),
                serde_yaml::Value::Number(n) => Some(n.to_string()),
                serde_yaml::Value::Bool(b) => Some(b.to_string()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// 规划导入：第一遍建映射，第二遍转换。同名清洗冲突自动 `-2` 后缀。
pub fn plan_import(
    md_files: Vec<SourceMd>,
    assets: Vec<AssetDest>,
    opts: &ImportOptions,
) -> ImportPlan {
    let mut plan = ImportPlan::default();

    // ── 第一遍：目标路径（清洗 + 去重）与解析映射 ────────────────────────
    let mut dest_of_rel: HashMap<String, String> = HashMap::new();
    let mut used_dests: std::collections::HashSet<String> = Default::default();
    for f in &md_files {
        let segments: Vec<String> = f.rel.split('/').map(sanitize_segment).collect();
        let mut dest = format!("{}/{}", opts.dest_root, segments.join("/"));
        if !dest.ends_with(".md") {
            dest.push_str(".md");
        }
        if !used_dests.insert(dest.clone()) {
            let mut n = 2;
            loop {
                let alt = dest.replace(".md", &format!("-{n}.md"));
                if used_dests.insert(alt.clone()) {
                    dest = alt;
                    break;
                }
                n += 1;
            }
        }
        dest_of_rel.insert(f.rel.clone(), dest);
    }

    // 解析映射：精确 rel（±.md）> 唯一 basename > 唯一 title；资源并入 basename 命名空间
    let mut by_exact: HashMap<String, String> = HashMap::new();
    let mut by_basename: HashMap<String, String> = HashMap::new();
    let mut ambiguous: std::collections::HashSet<String> = Default::default();
    let mut by_title: HashMap<String, String> = HashMap::new();
    for f in &md_files {
        let dest = dest_of_rel[&f.rel].clone();
        by_exact.insert(f.rel.clone(), dest.clone());
        by_exact
            .entry(f.rel.trim_end_matches(".md").to_string())
            .or_insert(dest.clone());
        record_unique(
            &mut by_basename,
            &mut ambiguous,
            &file_stem_of(&f.rel),
            dest.clone(),
        );
        if let Some(title) = extract_title(f) {
            record_unique(&mut by_title, &mut ambiguous, &title, dest);
        }
    }
    let mut asset_by_exact: HashMap<String, String> = HashMap::new();
    let mut asset_by_basename: HashMap<String, String> = HashMap::new();
    let mut asset_ambiguous: std::collections::HashSet<String> = Default::default();
    for a in &assets {
        asset_by_exact.insert(a.src_rel.clone(), a.dest.clone());
        let base = a
            .src_rel
            .rsplit('/')
            .next()
            .unwrap_or(&a.src_rel)
            .to_string();
        record_unique(
            &mut asset_by_basename,
            &mut asset_ambiguous,
            &base,
            a.dest.clone(),
        );
    }

    let resolve = |target: &str| -> Option<ResolveOut> {
        let t = target.trim().trim_start_matches("./");
        if let Some(d) = by_exact.get(t).or_else(|| by_exact.get(&format!("{t}.md"))) {
            return Some((d.clone(), false));
        }
        if let Some(d) = by_basename.get(file_stem_of(t).as_str()) {
            return Some((d.clone(), false));
        }
        if let Some(d) = by_title.get(t) {
            return Some((d.clone(), false));
        }
        if let Some(d) = asset_by_exact.get(t) {
            return Some((d.clone(), true));
        }
        if let Some(d) = asset_by_basename.get(t) {
            return Some((d.clone(), true));
        }
        None
    };

    // ── 第二遍：frontmatter 映射 + 链接重写 ─────────────────────────────
    let mut warned: std::collections::HashSet<String> = Default::default();
    let mut per_file_stats = WikiStats::default();
    for f in md_files {
        let stem = file_stem_of(&f.rel);
        let id = crate::id::new_note_id(chrono::Utc::now().naive_utc());
        let now = chrono::Utc::now()
            .format("%Y-%m-%dT%H:%M:%S+00:00")
            .to_string();
        let mapped = map_frontmatter(&f.text, &stem, &id, &now);
        // frontmatter 不含 wikilink，只重写 body 部分：按第二个 --- 拆分
        let (fm, body) = split_planned(&mapped);
        let mut st = WikiStats::default();
        let converted = convert_wikilinks(&body, &resolve, &mut st);
        // 未解析目标收集 warning（去重）
        for m in body_wikilink_targets(&body) {
            if resolve(&m).is_none() && warned.insert(m.clone()) {
                plan.warnings
                    .push(format!("未解析链接目标：[[{m}]]（保留原文）"));
            }
        }
        per_file_stats.resolved += st.resolved;
        per_file_stats.unresolved += st.unresolved;
        plan.notes.push(PlannedNote {
            dest: dest_of_rel[&f.rel].clone(),
            content: format!("{fm}{converted}"),
        });
    }
    plan.stats = per_file_stats;
    plan
}

/// 唯一性记录：重复 key 标记 ambiguous 并移除（宁可不解也不解错）。
fn record_unique(
    map: &mut HashMap<String, String>,
    ambiguous: &mut std::collections::HashSet<String>,
    key: &str,
    value: String,
) {
    if ambiguous.contains(key) {
        return;
    }
    if let Some(existing) = map.get(key) {
        if existing != &value {
            map.remove(key);
            ambiguous.insert(key.to_string());
        }
        return;
    }
    map.insert(key.to_string(), value);
}

/// 从源文本抽 title（无/坏 frontmatter → None）。
fn extract_title(f: &SourceMd) -> Option<String> {
    let (fm, _) = split_existing_frontmatter(&f.text);
    fm?.get("title")?.as_str().map(|s| s.to_string())
}

/// 拆已映射文本为（frontmatter 含结尾 --- 与空行，body）。
fn split_planned(mapped: &str) -> (String, String) {
    let rest = mapped.strip_prefix("---\n").unwrap_or(mapped);
    match rest.find("\n---\n") {
        Some(end) => (
            format!("---\n{}\n---\n", &rest[..end]),
            rest[end + 5..].to_string(),
        ),
        None => (String::new(), mapped.to_string()),
    }
}

/// 列出 body 中的 wikilink 目标（用于 warning 收集）。
fn body_wikilink_targets(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = body;
    while let Some(pos) = rest.find("[[") {
        let after = &rest[pos + 2..];
        match after.find("]]") {
            Some(end) => {
                let inner = &after[..end];
                let target = inner.split('|').next().unwrap_or("");
                let path = target.split('#').next().unwrap_or("");
                let p = path.trim().trim_start_matches("./");
                if !p.is_empty() {
                    out.push(p.to_string());
                }
                rest = &after[end + 2..];
            }
            None => break,
        }
    }
    out
}

// ── 内部工具 ──────────────────────────────────────────────────────────────

fn file_stem_of(rel: &str) -> String {
    let name = rel.rsplit('/').next().unwrap_or(rel);
    name.strip_suffix(".md").unwrap_or(name).to_string()
}

fn sanitize_segment(seg: &str) -> String {
    let cleaned: String = seg
        .chars()
        .map(|c| match c {
            '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => c,
        })
        .collect();
    let trimmed = cleaned.trim().trim_start_matches('.').to_string();
    if trimmed.is_empty() {
        "untitled".to_string()
    } else {
        trimmed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── convert_wikilinks ──────────────────────────────────────────────────

    #[test]
    fn wikilinks_basic_alias_and_path() {
        let mut stats = WikiStats::default();
        let resolver = |t: &str| -> Option<ResolveOut> {
            match t {
                "Note" | "note.md" => Some(("notes/import/note.md".into(), false)),
                "ideas/Deep Thought" => Some(("notes/import/ideas/Deep Thought.md".into(), false)),
                _ => None,
            }
        };
        let out = convert_wikilinks(
            "见 [[Note]] 与 [[Note|别名]]，还有 [[ideas/Deep Thought]]。",
            &resolver,
            &mut stats,
        );
        assert_eq!(
            out,
            "见 [Note](<notes/import/note.md>) 与 [别名](<notes/import/note.md>)，还有 [Deep Thought](<notes/import/ideas/Deep Thought.md>)。"
        );
        assert_eq!(
            stats,
            WikiStats {
                resolved: 3,
                unresolved: 0
            }
        );
    }

    #[test]
    fn wikilinks_unresolved_kept_verbatim() {
        let mut stats = WikiStats::default();
        let out = convert_wikilinks("悬空 [[Ghost]] 链接", &|_| None, &mut stats);
        assert_eq!(out, "悬空 [[Ghost]] 链接");
        assert_eq!(stats.unresolved, 1);
        assert_eq!(stats.resolved, 0);
    }

    #[test]
    fn wikilinks_embeds_and_headings() {
        let mut stats = WikiStats::default();
        let resolver = |t: &str| -> Option<ResolveOut> {
            match t {
                "pic.png" => Some(("/assets/img/ab/pic.png".into(), true)),
                "Note" => Some(("notes/import/note.md".into(), false)),
                _ => None,
            }
        };
        let out = convert_wikilinks(
            "嵌入 ![[pic.png]]，小节 [[Note#引言]]，笔记嵌入 ![[Note]]。",
            &resolver,
            &mut stats,
        );
        assert_eq!(
            out,
            "嵌入 ![](/assets/img/ab/pic.png)，小节 [Note](<notes/import/note.md>)，笔记嵌入 [Note](<notes/import/note.md>)。"
        );
        assert_eq!(
            stats,
            WikiStats {
                resolved: 3,
                unresolved: 0
            }
        );
    }

    // ── map_frontmatter ────────────────────────────────────────────────────

    #[test]
    fn frontmatter_generated_when_missing() {
        let out = map_frontmatter("正文", "我的笔记", "nt_x", "2026-09-05T10:00:00+08:00");
        assert!(out.starts_with("---\n"), "要有 frontmatter：{out}");
        assert!(out.contains("type: note\n"));
        assert!(out.contains("id: nt_x\n"));
        assert!(out.contains("title: 我的笔记\n"));
        assert!(out.contains("created: 2026-09-05T10:00:00+08:00\n"));
        assert!(
            out.ends_with("\n正文") || out.ends_with("---\n\n正文"),
            "body 保留：{out}"
        );
    }

    #[test]
    fn frontmatter_coerces_string_tags_and_aliases() {
        let raw = "---\ntitle: T\ntags: \"a, b\"\naliases: 单别名\n---\n\n正文";
        let out = map_frontmatter(raw, "file", "nt_x", "2026-09-05T10:00:00+08:00");
        // 结构化断言（与 yaml 序列化风格无关）："a, b"（字符串）→ [a, b]；"单别名" → [单别名]
        let (fm, _) = split_existing_frontmatter(&out);
        let fm = fm.expect("输出应含可解析 frontmatter");
        let list = |key: &str| -> Vec<String> {
            fm.get(key)
                .and_then(|v| {
                    v.as_sequence().map(|s| {
                        s.iter()
                            .filter_map(|i| i.as_str().map(|x| x.to_string()))
                            .collect()
                    })
                })
                .unwrap_or_default()
        };
        assert_eq!(
            list("tags"),
            vec!["a".to_string(), "b".to_string()],
            "{out}"
        );
        assert_eq!(list("aliases"), vec!["单别名".to_string()], "{out}");
    }

    #[test]
    fn frontmatter_keeps_type_and_unknown_keys() {
        let raw = "---\ntype: source\ndraft: true\n---\n\n正文";
        let out = map_frontmatter(raw, "file", "nt_x", "2026-09-05T10:00:00+08:00");
        assert!(out.contains("type: source\n"), "已有 type 保留");
        assert!(out.contains("draft: true"), "未知键保留：{out}");
        assert!(out.contains("id: nt_x\n"), "补 id");
    }

    // ── plan_import ────────────────────────────────────────────────────────

    #[test]
    fn plan_import_resolves_links_and_maps_assets() {
        let plan = plan_import(
            vec![
                SourceMd {
                    rel: "a.md".into(),
                    text: "---\ntitle: Alpha\n---\n\n链接 [[b]] 和图 ![[pic.png]]。".into(),
                },
                SourceMd {
                    rel: "sub/b.md".into(),
                    text: "回链 [[Alpha]]。".into(),
                },
            ],
            vec![AssetDest {
                src_rel: "pic.png".into(),
                dest: "/assets/img/ab/hash.png".into(),
            }],
            &ImportOptions {
                dest_root: "notes/import-20260905".into(),
            },
        );
        assert_eq!(plan.notes.len(), 2);
        let a = plan
            .notes
            .iter()
            .find(|n| n.dest.ends_with("a.md"))
            .unwrap();
        assert_eq!(a.dest, "notes/import-20260905/a.md");
        assert!(
            a.content.contains("[b](<notes/import-20260905/sub/b.md>)"),
            "按 basename 解析：{}",
            a.content
        );
        assert!(
            a.content.contains("![](/assets/img/ab/hash.png)"),
            "资源嵌入：{}",
            a.content
        );
        let b = plan
            .notes
            .iter()
            .find(|n| n.dest.ends_with("b.md"))
            .unwrap();
        assert!(
            b.content.contains("[Alpha](<notes/import-20260905/a.md>)"),
            "按 title 解析：{}",
            b.content
        );
        assert_eq!(
            plan.stats,
            WikiStats {
                resolved: 3,
                unresolved: 0
            }
        );
    }

    #[test]
    fn plan_import_sanitizes_and_dedups_collisions() {
        let plan = plan_import(
            vec![
                SourceMd {
                    rel: "a:b.md".into(),
                    text: "x".into(),
                }, // Windows 非法字符 → 清洗
                SourceMd {
                    rel: "a_b.md".into(),
                    text: "y".into(),
                }, // 清洗后同名 → -2
            ],
            vec![],
            &ImportOptions::default(),
        );
        let dests: Vec<&str> = plan.notes.iter().map(|n| n.dest.as_str()).collect();
        assert!(dests.contains(&"notes/import/a_b.md"), "{dests:?}");
        assert!(dests.contains(&"notes/import/a_b-2.md"), "{dests:?}");
    }

    #[test]
    fn plan_import_unresolved_links_warn() {
        let plan = plan_import(
            vec![SourceMd {
                rel: "a.md".into(),
                text: "[[ghost]]".into(),
            }],
            vec![],
            &ImportOptions::default(),
        );
        assert_eq!(plan.stats.unresolved, 1);
        assert!(
            plan.warnings.iter().any(|w| w.contains("ghost")),
            "未解析链接要出 warning：{:?}",
            plan.warnings
        );
    }
}
