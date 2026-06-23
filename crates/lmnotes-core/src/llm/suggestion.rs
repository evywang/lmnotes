//! 建议数据结构（ADR-0005 §6）。
//!
//! 序列化模型（评审 R1 修正）：`payload` 列存 `Suggestion` 的完整 serde JSON
//! （内部 tag，形如 `{"summary":{"text":"..."}}`）。读回时直接 `from_str(payload)`，
//! 不再重组。`kind` 列冗余存 `kind_str()` 仅供 SQL 查询/索引便利，反序列化不依赖它。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum Suggestion {
    #[serde(rename = "summary")]
    Summary { text: String },
    #[serde(rename = "tag")]
    Tag { tag: String },
    #[serde(rename = "link")]
    Link { dst_path: String, link_text: String },
}

impl Suggestion {
    pub fn kind_str(&self) -> &'static str {
        match self {
            Suggestion::Summary { .. } => "summary",
            Suggestion::Tag { .. } => "tag",
            Suggestion::Link { .. } => "link",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SuggestionStatus {
    Pending,
    Accepted,
    Rejected,
}

impl SuggestionStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            SuggestionStatus::Pending => "pending",
            SuggestionStatus::Accepted => "accepted",
            SuggestionStatus::Rejected => "rejected",
        }
    }
    pub fn parse(s: &str) -> Self {
        match s {
            "accepted" => Self::Accepted,
            "rejected" => Self::Rejected,
            _ => Self::Pending,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuggestionRecord {
    pub id: String,
    pub concept_id: String,
    pub suggestion: Suggestion,
    pub status: SuggestionStatus,
}
