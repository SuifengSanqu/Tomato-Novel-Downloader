//! 多平台小说下载抽象层。
//!
//! 本模块定义统一的 `NovelPlatform` trait,所有具体平台(番茄、七猫等)通过实现该 trait 接入。
//! 上层搜索/下载/生成管线只依赖该 trait,不感知具体平台。
//!
//! 模块结构:
//! - `model`  — 平台无关的共享数据模型
//! - `registry` — 平台注册表,支持多平台并行搜索聚合
//! - `fanqie`  — 番茄小说适配器(基于 tomato-novel-official-api)
//! - `qimao`   — 七猫免费小说适配器(基于 web scraping)
//! - `scraper` — 通用 web 爬取工具

pub mod model;
pub mod registry;

#[cfg(feature = "official-api")]
pub mod fanqie;

pub mod qimao;
pub mod scraper;

pub use model::{ChapterContent, ChapterState, NovelDetail, NovelId, PlatformId, PlatformMeta, SearchResult};
pub use registry::PlatformRegistry;

use anyhow::Result;

/// 统一的小说平台接口。每个平台一个实现。
pub trait NovelPlatform: Send + Sync {
    /// 平台唯一标识(如 "fanqie"、"qimao")。
    fn id(&self) -> PlatformId;

    /// 人类可读的平台名称(如 "番茄小说")。
    fn meta(&self) -> &PlatformMeta;

    /// 按关键字搜索作品,返回候选列表。
    fn search(&self, keyword: &str) -> Result<Vec<SearchResult>>;

    /// 按平台内作品 ID 获取详情(元数据 + 完整目录)。
    fn fetch_detail(&self, novel_id: &str) -> Result<NovelDetail>;

    /// 抓取单个章节正文。返回值携带状态标识:
    /// - `Ok`: 可读章节,正文可用
    /// - `Locked`: 需广告解锁/会员/付费,后端无法合法获取
    /// - `Failed`: 网络或解析错误
    fn fetch_chapter(&self, novel_id: &str, chapter_id: &str, chapter_title: &str) -> Result<ChapterContent>;

    /// 该平台是否完全免费(番茄/七猫等广告模式)。
    /// 用于决定受限章节的 UI 提示文案。
    fn is_free_platform(&self) -> bool {
        false
    }
}

