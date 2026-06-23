//! 番茄小说适配器。
//!
//! 封装 `tomato-novel-official-api` 库,实现 `NovelPlatform` trait。
//! 番茄是广告解锁的免费阅读平台,所有章节原则上可免费获取(需有效会话)。

use anyhow::{Context, Result};
use tomato_novel_official_api::{DirectoryClient, SearchClient};

use crate::base_system::cooldown_retry::fetch_with_cooldown_retry;
use crate::base_system::context::Config;

use super::model::{
    ChapterContent, ChapterRef, ChapterState, NovelDetail, NovelId, PlatformMeta, SearchResult,
};
use super::NovelPlatform;

static META: PlatformMeta = PlatformMeta {
    id: "fanqie",
    name: "番茄小说",
    domain: "fanqienovel.com",
    requires_auth: false,
    is_free: true,
};

pub struct FanqiePlatform {
    config: Config,
}

impl FanqiePlatform {
    pub fn new() -> Self {
        Self {
            config: Config::default(),
        }
    }
}

impl NovelPlatform for FanqiePlatform {
    fn id(&self) -> super::PlatformId {
        "fanqie"
    }

    fn meta(&self) -> &PlatformMeta {
        &META
    }

    fn is_free_platform(&self) -> bool {
        true
    }

    fn search(&self, keyword: &str) -> Result<Vec<SearchResult>> {
        let client = SearchClient::new().context("初始化番茄搜索客户端")?;
        let resp = client
            .search_books(keyword)
            .context("番茄小说搜索失败")?;

        Ok(resp
            .books
            .into_iter()
            .map(|b| SearchResult {
                id: NovelId::new("fanqie", b.book_id.clone()),
                title: b.title,
                author: b.author,
                platform_name: "番茄小说".to_string(),
                intro: None,
                cover_url: None,
                chapter_count: None,
                finished: None,
            })
            .collect())
    }

    fn fetch_detail(&self, novel_id: &str) -> Result<NovelDetail> {
        let directory = DirectoryClient::new().context("初始化番茄目录客户端")?;
        let dir = directory
            .fetch_directory_with_cover(novel_id, None, None)
            .context("获取番茄小说目录失败")?;

        let chapters: Vec<ChapterRef> = dir
            .chapters
            .into_iter()
            .enumerate()
            .map(|(i, ch)| ChapterRef {
                id: ch.id,
                title: ch.title,
                index: i as u32,
            })
            .collect();

        let book_name = dir.meta.book_name;
        let author = dir.meta.author;

        Ok(NovelDetail {
            id: NovelId::new("fanqie", novel_id.to_string()),
            title: book_name.unwrap_or_else(|| novel_id.to_string()),
            author: author.unwrap_or_default(),
            intro: dir.meta.description,
            cover_url: dir
                .meta
                .cover_path
                .as_ref()
                .map(|p| p.to_string_lossy().to_string())
                .or(dir.meta.cover_url),
            tags: dir.meta.tags,
            finished: dir.meta.finished,
            chapter_count: dir.meta.chapter_count.map(|c| c as u32),
            word_count: dir.meta.word_count.map(|w| w as u32),
            chapters,
        })
    }

    fn fetch_chapter(
        &self,
        novel_id: &str,
        chapter_id: &str,
        chapter_title: &str,
    ) -> Result<ChapterContent> {
        use tomato_novel_official_api::FanqieClient;

        let client = FanqieClient::new().context("初始化番茄内容客户端")?;

        // 正文内容使用现有 fetch_with_cooldown_retry(同下载管线)
        match fetch_with_cooldown_retry(&client, chapter_id, false, Some(novel_id)) {
            Ok(value) => {
                // 从返回的 JSON 中提取正文
                let content = value["data"]["contents"]
                    .as_array()
                    .and_then(|arr| {
                        arr.iter()
                            .find(|item| item["item_id"].as_str() == Some(chapter_id))
                    })
                    .and_then(|item| item["content"].as_str())
                    .map(|s| s.to_string());

                match content {
                    Some(text) if !text.trim().is_empty() => {
                        Ok(ChapterContent::ok(chapter_id, chapter_title, text))
                    }
                    Some(_) => Ok(ChapterContent::locked(
                        chapter_id,
                        chapter_title,
                        "章节内容为空,可能需要看广告解锁",
                    )),
                    None => Ok(ChapterContent::failed(
                        chapter_id,
                        chapter_title,
                        "响应中未找到对应章节内容",
                    )),
                }
            }
            Err(e) => {
                let msg = e.to_string();
                if msg.contains("Cooldown") || msg.contains("cooldown") {
                    Ok(ChapterContent::locked(
                        chapter_id,
                        chapter_title,
                        format!("触发频率限制: {msg}"),
                    ))
                } else {
                    Ok(ChapterContent::failed(
                        chapter_id,
                        chapter_title,
                        format!("获取失败: {msg}"),
                    ))
                }
            }
        }
    }
}
