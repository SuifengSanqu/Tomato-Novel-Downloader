//! 平台无关的共享数据模型。
//!
//! 这些类型独立于任何具体平台,用于上层的搜索、目录展示和下载管线。

use serde::{Deserialize, Serialize};

/// 平台标识符,稳定的短字符串(如 "fanqie"、"qimao")。
pub type PlatformId = &'static str;

/// 平台的元信息描述。
#[derive(Debug, Clone)]
pub struct PlatformMeta {
    pub id: PlatformId,
    pub name: &'static str,
    pub domain: &'static str,
    /// 是否需要用户登录/Cookie 才能下载正文
    pub requires_auth: bool,
    /// 该平台是否完全免费(广告模式,非付费墙模式)
    pub is_free: bool,
}

/// 作品唯一标识。格式: "{platform}:{novel_id}"
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NovelId {
    pub platform: String,
    pub raw: String,
}

impl NovelId {
    pub fn new(platform: impl Into<String>, raw: impl Into<String>) -> Self {
        Self {
            platform: platform.into(),
            raw: raw.into(),
        }
    }

    /// 解析 "platform:raw" 格式的 ID。
    pub fn parse(combined: &str) -> Option<Self> {
        let pos = combined.find(':')?;
        let platform = combined[..pos].to_string();
        let raw = combined[pos + 1..].to_string();
        if platform.is_empty() || raw.is_empty() {
            return None;
        }
        Some(Self { platform, raw })
    }
}

impl std::fmt::Display for NovelId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.platform, self.raw)
    }
}

/// 搜索命中的单个作品摘要。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub id: NovelId,
    pub title: String,
    pub author: String,
    pub platform_name: String,
    pub intro: Option<String>,
    pub cover_url: Option<String>,
    pub chapter_count: Option<u32>,
    pub finished: Option<bool>,
}

/// 作品详情:元数据 + 完整目录。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NovelDetail {
    pub id: NovelId,
    pub title: String,
    pub author: String,
    pub intro: Option<String>,
    pub cover_url: Option<String>,
    pub tags: Vec<String>,
    pub finished: Option<bool>,
    pub chapter_count: Option<u32>,
    pub word_count: Option<u32>,
    pub chapters: Vec<ChapterRef>,
}

/// 目录中的单章引用。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChapterRef {
    pub id: String,
    pub title: String,
    pub index: u32,
}

/// 章节抓取后的状态。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChapterState {
    /// 正常取到正文。
    Ok,
    /// 受限:需广告解锁或会员/付费,后端无法合法获取。
    Locked {
        reason: String,
    },
    /// 抓取失败(网络/解析错误)。
    Failed {
        reason: String,
    },
}

/// 章节正文及其状态。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChapterContent {
    pub chapter_id: String,
    pub chapter_title: String,
    pub state: ChapterState,
    pub body: Option<String>,
}

impl ChapterContent {
    pub fn ok(chapter_id: impl Into<String>, chapter_title: impl Into<String>, body: String) -> Self {
        Self {
            chapter_id: chapter_id.into(),
            chapter_title: chapter_title.into(),
            state: ChapterState::Ok,
            body: Some(body),
        }
    }

    pub fn locked(
        chapter_id: impl Into<String>,
        chapter_title: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            chapter_id: chapter_id.into(),
            chapter_title: chapter_title.into(),
            state: ChapterState::Locked {
                reason: reason.into(),
            },
            body: None,
        }
    }

    pub fn failed(
        chapter_id: impl Into<String>,
        chapter_title: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            chapter_id: chapter_id.into(),
            chapter_title: chapter_title.into(),
            state: ChapterState::Failed {
                reason: reason.into(),
            },
            body: None,
        }
    }
}
