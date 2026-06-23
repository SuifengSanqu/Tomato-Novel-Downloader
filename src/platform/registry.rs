//! 平台注册表:管理所有已接入的平台实例。
//!
//! 支持按平台 ID 取用、列出所有平台、以及多平台并行搜索聚合。

use std::collections::BTreeMap;
use std::sync::Arc;

use anyhow::{Result, anyhow};
use tracing::warn;

use super::{NovelPlatform, PlatformId, SearchResult};

/// 平台注册表。启动时注册所有可用平台,供管线按需调度。
#[derive(Clone, Default)]
pub struct PlatformRegistry {
    platforms: BTreeMap<PlatformId, Arc<dyn NovelPlatform>>,
}

impl PlatformRegistry {
    pub fn new() -> Self {
        Self {
            platforms: BTreeMap::new(),
        }
    }

    /// 注册一个平台。
    pub fn register(&mut self, platform: Arc<dyn NovelPlatform>) {
        self.platforms.insert(platform.id(), platform);
    }

    /// 按 ID 取平台。
    pub fn get(&self, id: &str) -> Result<Arc<dyn NovelPlatform>> {
        self.platforms
            .get(id)
            .cloned()
            .ok_or_else(|| anyhow!("未知平台: {id}"))
    }

    /// 列出所有已注册平台的 (id, 显示名, 域名)。
    pub fn list_all(&self) -> Vec<(PlatformId, String, String)> {
        self.platforms
            .values()
            .map(|p| {
                let m = p.meta();
                (p.id(), m.name.to_string(), m.domain.to_string())
            })
            .collect()
    }

    /// 列出所有已注册平台的 id。
    pub fn ids(&self) -> Vec<PlatformId> {
        self.platforms.keys().copied().collect()
    }

    /// 平台总数。
    pub fn len(&self) -> usize {
        self.platforms.len()
    }

    pub fn is_empty(&self) -> bool {
        self.platforms.is_empty()
    }

    /// 在指定平台(为空则全部)上搜索,聚合结果。
    /// 单个平台失败不影响其他平台,错误降级为空结果并记录日志。
    pub fn search_across(&self, keyword: &str, only: &[String]) -> Vec<SearchResult> {
        let targets: Vec<&Arc<dyn NovelPlatform>> = if only.is_empty() {
            self.platforms.values().collect()
        } else {
            only.iter()
                .filter_map(|id| self.platforms.get(id.as_str()))
                .collect()
        };

        let mut hits = Vec::new();
        for platform in targets {
            match platform.search(keyword) {
                Ok(mut found) => hits.append(&mut found),
                Err(e) => {
                    warn!(
                        platform = platform.id(),
                        error = %e,
                        "平台搜索失败,已跳过"
                    );
                }
            }
        }
        hits
    }

    /// 根据 NovelId 自动路由到对应平台获取详情。
    pub fn fetch_detail(&self, novel_id: &crate::platform::NovelId) -> Result<crate::platform::NovelDetail> {
        let platform = self.get(&novel_id.platform)?;
        platform.fetch_detail(&novel_id.raw)
    }

    /// 根据 NovelId 自动路由到对应平台抓取章节。
    pub fn fetch_chapter(
        &self,
        novel_id: &crate::platform::NovelId,
        chapter_id: &str,
        chapter_title: &str,
    ) -> Result<crate::platform::ChapterContent> {
        let platform = self.get(&novel_id.platform)?;
        platform.fetch_chapter(&novel_id.raw, chapter_id, chapter_title)
    }
}
