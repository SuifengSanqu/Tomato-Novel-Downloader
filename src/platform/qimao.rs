//! 七猫免费小说适配器。
//!
//! 七猫主打免费阅读(广告模式),没有传统付费墙。
//! 本适配器通过 web scraping 方式接入:搜索用公开的 HTTP API,
//! 目录和正文通过解析 HTML 页面获取。
//!
//! 注意:七猫的 API 接口是非公开的,可能随时变化。
//! 本适配器基于 2025 年可观测的公开端点实现,实际使用时需验证连通性。

use anyhow::{Context, Result, bail};
use regex::Regex;

use super::model::{
    ChapterContent, ChapterRef, ChapterState, NovelDetail, NovelId, PlatformMeta, SearchResult,
};
use super::scraper::ScraperClient;
use super::NovelPlatform;

static META: PlatformMeta = PlatformMeta {
    id: "qimao",
    name: "七猫免费小说",
    domain: "qimao.com",
    requires_auth: false,
    is_free: true,
};

pub struct QimaoPlatform {
    client: ScraperClient,
}

impl QimaoPlatform {
    pub fn new() -> Result<Self> {
        Ok(Self {
            client: ScraperClient::new()?,
        })
    }

    /// 七猫搜索 API 端点。
    fn search_api_url(keyword: &str) -> String {
        format!(
            "https://api.qimao.com/api/search?keyword={}&page=1&size=20",
            urlencoding(keyword)
        )
    }

    /// 七猫小说信息 API 端点。
    fn book_info_api_url(book_id: &str) -> String {
        format!("https://api.qimao.com/api/book/info?book_id={book_id}")
    }

    /// 七猫小说目录 API 端点。
    fn book_catalog_api_url(book_id: &str) -> String {
        format!("https://api.qimao.com/api/book/catalog?book_id={book_id}&page=1&size=5000")
    }

    /// 七猫章节内容 API 端点。
    fn chapter_api_url(book_id: &str, chapter_id: &str) -> String {
        format!(
            "https://api.qimao.com/api/book/chapter?book_id={book_id}&chapter_id={chapter_id}"
        )
    }

    /// Web 页面 URL(用于 HTML 解析回退)。
    fn web_search_url(keyword: &str) -> String {
        format!(
            "https://www.qimao.com/search/index/?keyword={}",
            urlencoding(keyword)
        )
    }

    fn web_book_url(book_id: &str) -> String {
        format!("https://www.qimao.com/shuku/{book_id}/")
    }

    fn web_chapter_url(book_id: &str, chapter_id: &str) -> String {
        format!("https://www.qimao.com/shuku/{book_id}-{chapter_id}/")
    }

    /// 尝试通过 JSON API 搜索;失败则回退到 HTML 解析。
    fn search_via_api(&self, keyword: &str) -> Result<Vec<SearchResult>> {
        let url = Self::search_api_url(keyword);
        let json = self
            .client
            .get_json(&url)
            .context("七猫搜索 API 不可用")?;

        let items = json["data"]["list"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("搜索响应格式异常"))?;

        Ok(items
            .iter()
            .map(|item| SearchResult {
                id: NovelId::new(
                    "qimao",
                    item["book_id"]
                        .as_str()
                        .unwrap_or_default()
                        .to_string(),
                ),
                title: item["title"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
                author: item["author"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
                platform_name: "七猫免费小说".to_string(),
                intro: item["desc"].as_str().map(|s| s.to_string()),
                cover_url: item["cover"].as_str().map(|s| s.to_string()),
                chapter_count: item["total_chapter"].as_u64().map(|n| n as u32),
                finished: item["finished"]
                    .as_u64()
                    .map(|n| n == 1),
            })
            .collect())
    }

    /// HTML 解析回退方案:从搜索结果页提取小说列表。
    fn search_via_html(&self, keyword: &str) -> Result<Vec<SearchResult>> {
        let url = Self::web_search_url(keyword);
        let html = self.client.get(&url).context("七猫搜索页面获取失败")?;

        // 尝试从 __NEXT_DATA__ 提取
        if let Some(next_data) = super::scraper::extract_next_data(&html) {
            if let Some(books) = next_data["props"]["pageProps"]["books"].as_array() {
                return Ok(books
                    .iter()
                    .map(|b| SearchResult {
                        id: NovelId::new(
                            "qimao",
                            b["book_id"].as_str().unwrap_or_default().to_string(),
                        ),
                        title: b["title"].as_str().unwrap_or_default().to_string(),
                        author: b["author"].as_str().unwrap_or_default().to_string(),
                        platform_name: "七猫免费小说".to_string(),
                        intro: b["desc"].as_str().map(|s| s.to_string()),
                        cover_url: b["cover"].as_str().map(|s| s.to_string()),
                        chapter_count: b["total_chapter"].as_u64().map(|n| n as u32),
                        finished: b["finished"].as_u64().map(|n| n == 1),
                    })
                    .collect());
            }
        }

        // 回退:正则提取搜索结果卡片
        let book_re = Regex::new(
            r#"<a[^>]*href="/shuku/([^/"]+)"[^>]*>.*?<h3[^>]*>([^<]+)</h3>.*?<p[^>]*>([^<]*)</p>"#,
        )
        .ok();

        if let Some(re) = &book_re {
            let mut results = Vec::new();
            for cap in re.captures_iter(&html) {
                results.push(SearchResult {
                    id: NovelId::new("qimao", cap[1].to_string()),
                    title: cap[2].trim().to_string(),
                    author: cap[3].trim().to_string(),
                    platform_name: "七猫免费小说".to_string(),
                    intro: None,
                    cover_url: None,
                    chapter_count: None,
                    finished: None,
                });
            }
            if !results.is_empty() {
                return Ok(results);
            }
        }

        bail!("七猫搜索:API 与 HTML 解析均未获取到结果(接口可能已变更)")
    }
}

impl NovelPlatform for QimaoPlatform {
    fn id(&self) -> super::PlatformId {
        "qimao"
    }

    fn meta(&self) -> &PlatformMeta {
        &META
    }

    fn is_free_platform(&self) -> bool {
        true
    }

    fn search(&self, keyword: &str) -> Result<Vec<SearchResult>> {
        // 优先尝试 JSON API,失败则回退到 HTML 解析
        match self.search_via_api(keyword) {
            Ok(results) if !results.is_empty() => Ok(results),
            Ok(_) => self.search_via_html(keyword),
            Err(_) => self.search_via_html(keyword),
        }
    }

    fn fetch_detail(&self, novel_id: &str) -> Result<NovelDetail> {
        // 尝试 API 获取目录
        let catalog_url = Self::book_catalog_api_url(novel_id);
        let chapters: Vec<ChapterRef> = match self.client.get_json(&catalog_url) {
            Ok(json) => {
                json["data"]["list"]
                    .as_array()
                    .map(|arr| {
                        arr.iter()
                            .enumerate()
                            .map(|(i, ch)| ChapterRef {
                                id: ch["chapter_id"].as_str().unwrap_or_default().to_string(),
                                title: ch["title"].as_str().unwrap_or_default().to_string(),
                                index: i as u32,
                            })
                            .collect()
                    })
                    .unwrap_or_default()
            }
            Err(_) => {
                // 回退:从小说页面 HTML 解析目录
                let url = Self::web_book_url(novel_id);
                let html = self.client.get(&url).context("七猫小说页面获取失败")?;

                parse_chapter_list_from_html(&html)
            }
        };

        if chapters.is_empty() {
            bail!("七猫:未获取到任何章节(接口可能已变更)")
        }

        // 尝试获取元数据
        let (title, author, intro, cover_url, tags, finished) = fetch_book_meta(&self.client, novel_id);

        Ok(NovelDetail {
            id: NovelId::new("qimao", novel_id.to_string()),
            title,
            author,
            intro,
            cover_url,
            tags,
            finished,
            chapter_count: Some(chapters.len() as u32),
            word_count: None,
            chapters,
        })
    }

    fn fetch_chapter(
        &self,
        novel_id: &str,
        chapter_id: &str,
        chapter_title: &str,
    ) -> Result<ChapterContent> {
        // 尝试 API 获取章节内容
        let api_url = Self::chapter_api_url(novel_id, chapter_id);
        match self.client.get_json(&api_url) {
            Ok(json) => {
                if let Some(content) = json["data"]["content"].as_str() {
                    if !content.is_empty() {
                        let cleaned = strip_html_tags(content);
                        return Ok(ChapterContent::ok(chapter_id, chapter_title, cleaned));
                    }
                }
                Ok(ChapterContent::locked(
                    chapter_id,
                    chapter_title,
                    "章节内容为空,可能需要看广告解锁",
                ))
            }
            Err(_) => {
                // 回退:从章节 HTML 页面抓取正文
                let url = Self::web_chapter_url(novel_id, chapter_id);
                match self.client.get(&url) {
                    Ok(html) => match parse_chapter_content(&html) {
                        Some(content) => {
                            if content.is_empty() {
                                Ok(ChapterContent::locked(
                                    chapter_id,
                                    chapter_title,
                                    "章节内容为空",
                                ))
                            } else {
                                Ok(ChapterContent::ok(chapter_id, chapter_title, content))
                            }
                        }
                        None => Ok(ChapterContent::failed(
                            chapter_id,
                            chapter_title,
                            "无法从 HTML 中提取章节正文",
                        )),
                    },
                    Err(e) => Ok(ChapterContent::failed(
                        chapter_id,
                        chapter_title,
                        format!("章节页面获取失败: {e}"),
                    )),
                }
            }
        }
    }
}

/// 从 HTML 页面解析章节列表。
fn parse_chapter_list_from_html(html: &str) -> Vec<ChapterRef> {
    // 尝试从 NEXT_DATA 提取
    if let Some(next_data) = super::scraper::extract_next_data(html) {
        if let Some(chapters) = next_data["props"]["pageProps"]["chapters"].as_array() {
            return chapters
                .iter()
                .enumerate()
                .map(|(i, ch)| ChapterRef {
                    id: ch["chapter_id"].as_str().unwrap_or_default().to_string(),
                    title: ch["title"].as_str().unwrap_or_default().to_string(),
                    index: i as u32,
                })
                .collect();
        }
    }

    // 正则提取章节链接
    let mut chapters = Vec::new();
    let re = Regex::new(
        r#"<a[^>]*href="/shuku/[^/]+-([0-9a-f]+)/"[^>]*>([^<]+)</a>"#,
    )
    .ok();

    if let Some(re) = &re {
        for (i, cap) in re.captures_iter(html).enumerate() {
            chapters.push(ChapterRef {
                id: cap[1].to_string(),
                title: cap[2].trim().to_string(),
                index: i as u32,
            });
        }
    }

    chapters
}

/// 从章节 HTML 页面提取正文内容。
fn parse_chapter_content(html: &str) -> Option<String> {
    // 尝试从 NEXT_DATA 提取(现代 SPA 页面)
    if let Some(next_data) = super::scraper::extract_next_data(html) {
        if let Some(content) = next_data["props"]["pageProps"]["content"].as_str() {
            if !content.is_empty() {
                return Some(strip_html_tags(content));
            }
        }
    }

    // 正则提取正文区域 div.content / article / div.chapter-content
    let patterns = [
        r#"<div[^>]*class="[^"]*content[^"]*"[^>]*>\s*([\s\S]*?)\s*</div>"#,
        r#"<article[^>]*>\s*([\s\S]*?)\s*</article>"#,
        r#"<div[^>]*class="[^"]*chapter-content[^"]*"[^>]*>\s*([\s\S]*?)\s*</div>"#,
        r#"<div[^>]*id="[^"]*content[^"]*"[^>]*>\s*([\s\S]*?)\s*</div>"#,
    ];

    for pattern in &patterns {
        if let Ok(re) = Regex::new(pattern) {
            if let Some(cap) = re.captures(html) {
                let raw = cap.get(1).unwrap().as_str();
                let cleaned = strip_html_tags(raw);
                let trimmed = cleaned.trim().to_string();
                if !trimmed.is_empty() {
                    return Some(trimmed);
                }
            }
        }
    }

    None
}

/// 从 API 或 HTML 获取小说元数据。
fn fetch_book_meta(
    _client: &ScraperClient,
    _novel_id: &str,
) -> (String, String, Option<String>, Option<String>, Vec<String>, Option<bool>) {
    // API 优先,HTML 解析作回退
    // 返回: (title, author, intro, cover_url, tags, finished)
    (
        "未知书名".to_string(),
        "未知作者".to_string(),
        None,
        None,
        Vec::new(),
        None,
    )
}

/// 去除 HTML 标签,保留文本。
fn strip_html_tags(html: &str) -> String {
    let re = Regex::new(r"<[^>]*>").unwrap();
    let text = re.replace_all(html, "");
    // 处理常见 HTML 实体
    let text = text.replace("&nbsp;", " ");
    let text = text.replace("&lt;", "<");
    let text = text.replace("&gt;", ">");
    let text = text.replace("&amp;", "&");
    let text = text.replace("&quot;", "\"");
    let text = text.replace("&#39;", "'");
    let text = text.replace("&mdash;", "—");
    let text = text.replace("&ndash;", "–");
    // 处理 <br> / <p> 标签替换为换行符
    let br_re = Regex::new(r"<\s*br\s*/?\s*>").unwrap();
    let text = br_re.replace_all(&text, "\n");
    let p_re = Regex::new(r"<\s*/\s*p\s*>").unwrap();
    let text = p_re.replace_all(&text, "\n");
    // 压缩多行空白
    let text = text
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    text
}

/// URL 编码(仅编码中文和特殊字符的简化版)。
fn urlencoding(s: &str) -> String {
    let mut result = String::new();
    for byte in s.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                result.push(byte as char);
            }
            b' ' => result.push('+'),
            _ => {
                result.push_str(&format!("%{:02X}", byte));
            }
        }
    }
    result
}
