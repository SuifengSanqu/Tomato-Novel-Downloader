//! 番茄小说适配器。
//!
//! 通过直接 HTTP 调用番茄小说公开 API 实现搜索、目录和正文抓取。
//! 番茄是广告解锁的免费阅读平台,所有章节原则上可免费获取(需有效会话 Cookie)。

use anyhow::{Context, Result, bail};
use regex::Regex;
use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, CONTENT_TYPE, REFERER, USER_AGENT};
use serde_json::Value;
use std::time::Duration;

use super::model::{
    ChapterContent, ChapterRef, NovelDetail, NovelId, PlatformMeta, SearchResult,
};
use super::NovelPlatform;

const AID: &str = "1967";
const DEFAULT_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36";

static META: PlatformMeta = PlatformMeta {
    id: "fanqie",
    name: "番茄小说",
    domain: "fanqienovel.com",
    requires_auth: false,
    is_free: true,
};

pub struct FanqiePlatform {
    client: Client,
}

impl FanqiePlatform {
    pub fn new() -> Result<Self> {
        let mut headers = HeaderMap::new();
        headers.insert(USER_AGENT, HeaderValue::from_static(DEFAULT_UA));
        headers.insert(
            ACCEPT,
            HeaderValue::from_static("application/json, text/plain, */*"),
        );
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

        let client = Client::builder()
            .default_headers(headers)
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(15))
            .build()
            .context("创建番茄 HTTP 客户端失败")?;

        Ok(Self { client })
    }

    fn json_headers(&self, referer: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(USER_AGENT, HeaderValue::from_static(DEFAULT_UA));
        h.insert(
            ACCEPT,
            HeaderValue::from_static("application/json, text/plain, */*"),
        );
        if let Ok(v) = HeaderValue::from_str(referer) {
            h.insert(REFERER, v);
        }
        h
    }

    fn get_json(&self, url: &str, referer: &str) -> Result<Value> {
        let resp = self
            .client
            .get(url)
            .headers(self.json_headers(referer))
            .send()
            .with_context(|| format!("HTTP GET {url} 失败"))?;

        let status = resp.status();
        if !status.is_success() {
            bail!("HTTP GET {url} 返回 {status}");
        }

        resp.json()
            .with_context(|| format!("解析 {url} JSON 失败"))
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
        let encoded = urlencoding(keyword);
        let url = format!(
            "https://fanqienovel.com/api/author/search/search_book/v1?filter=127,127,127&page_index=0&page_size=20&query={encoded}"
        );
        let json = self
            .get_json(&url, "https://fanqienovel.com/")
            .context("番茄小说搜索失败")?;

        let list = json["data"]["search_book_list"]
            .as_array()
            .or_else(|| json["search_book_list"].as_array())
            .ok_or_else(|| anyhow::anyhow!("番茄搜索响应格式异常"))?;

        Ok(list
            .iter()
            .map(|b| SearchResult {
                id: NovelId::new(
                    "fanqie",
                    b["book_id"].as_str().unwrap_or_default().to_string(),
                ),
                title: b["book_name"]
                    .as_str()
                    .or_else(|| b["title"].as_str())
                    .unwrap_or_default()
                    .to_string(),
                author: b["author"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
                platform_name: "番茄小说".to_string(),
                intro: b["abstract"]
                    .as_str()
                    .or_else(|| b["description"].as_str())
                    .map(|s| s.to_string()),
                cover_url: b["thumb_url"]
                    .as_str()
                    .or_else(|| b["cover_url"].as_str())
                    .map(|s| s.to_string()),
                chapter_count: b["total_chapter"]
                    .as_u64()
                    .map(|n| n as u32),
                finished: b["creation_status"]
                    .as_str()
                    .map(|s| s == "2"),
            })
            .collect())
    }

    fn fetch_detail(&self, novel_id: &str) -> Result<NovelDetail> {
        let api_url = format!(
            "https://fanqienovel.com/api/reader/directory/detail?bookId={novel_id}"
        );
        let referer = format!("https://fanqienovel.com/page/{novel_id}");

        let data = self
            .get_json(&api_url, &referer)
            .context("获取番茄小说目录失败")?;

        let chapters: Vec<ChapterRef> = parse_chapter_list(&data, novel_id);

        if chapters.is_empty() {
            bail!("番茄:未获取到任何章节(接口可能已变更)")
        }

        let (title, author, intro, cover_url, tags, finished, chapter_count) =
            parse_book_meta(&self.client, novel_id, &data);

        Ok(NovelDetail {
            id: NovelId::new("fanqie", novel_id.to_string()),
            title,
            author,
            intro,
            cover_url,
            tags,
            finished,
            chapter_count,
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
        let url = format!(
            "https://novel.snssdk.com/api/novel/book/reader/full/v1/?item_id={chapter_id}&aid={AID}"
        );
        let referer = format!("https://fanqienovel.com/reader/{novel_id}");

        let mut delay = Duration::from_millis(1100);
        let mut last_error: Option<String> = None;

        for _attempt in 0..6 {
            match self.get_json(&url, &referer) {
                Ok(value) => {
                    let content = extract_chapter_text(&value, chapter_id, novel_id);
                    match content {
                        Some(text) if !text.trim().is_empty() => {
                            return Ok(ChapterContent::ok(chapter_id, chapter_title, text));
                        }
                        Some(_) => {
                            return Ok(ChapterContent::locked(
                                chapter_id,
                                chapter_title,
                                "章节内容为空,可能需要看广告解锁",
                            ));
                        }
                        None => {
                            return Ok(ChapterContent::failed(
                                chapter_id,
                                chapter_title,
                                "响应中未找到对应章节内容",
                            ));
                        }
                    }
                }
                Err(e) => {
                    let msg = e.to_string();
                    last_error = Some(msg.clone());
                    if msg.contains("Cooldown") || msg.contains("cooldown") || msg.contains("429")
                    {
                        std::thread::sleep(delay);
                        delay = std::cmp::min(delay * 2, Duration::from_secs(8));
                        continue;
                    }
                    return Ok(ChapterContent::failed(
                        chapter_id,
                        chapter_title,
                        format!("获取失败: {msg}"),
                    ));
                }
            }
        }

        Ok(ChapterContent::failed(
            chapter_id,
            chapter_title,
            format!("Cooldown 重试耗尽: {}", last_error.unwrap_or_default()),
        ))
    }
}

fn parse_chapter_list(data: &Value, _novel_id: &str) -> Vec<ChapterRef> {
    let vol = data["data"]["chapterListWithVolume"]
        .as_array()
        .or_else(|| data["chapterListWithVolume"].as_array());

    if let Some(volumes) = vol {
        let mut chapters = Vec::new();
        let mut idx = 0u32;
        for vol in volumes {
            if let Some(list) = vol["chapterList"].as_array() {
                for ch in list {
                    chapters.push(ChapterRef {
                        id: ch["itemId"]
                            .as_str()
                            .or_else(|| ch["item_id"].as_str())
                            .unwrap_or_default()
                            .to_string(),
                        title: ch["title"].as_str().unwrap_or_default().to_string(),
                        index: idx,
                    });
                    idx += 1;
                }
            }
        }
        if !chapters.is_empty() {
            return chapters;
        }
    }

    let flat = data["data"]["chapter_list"]
        .as_array()
        .or_else(|| data["chapter_list"].as_array());

    if let Some(list) = flat {
        return list
            .iter()
            .enumerate()
            .map(|(i, ch)| ChapterRef {
                id: ch["item_id"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
                title: ch["title"].as_str().unwrap_or_default().to_string(),
                index: i as u32,
            })
            .collect();
    }

    Vec::new()
}

fn parse_book_meta(
    client: &Client,
    novel_id: &str,
    dir_data: &Value,
) -> (
    String,
    String,
    Option<String>,
    Option<String>,
    Vec<String>,
    Option<bool>,
    Option<u32>,
) {
    let title = dir_data["data"]["novel"]["book_name"]
        .as_str()
        .or_else(|| dir_data["novel"]["book_name"].as_str())
        .unwrap_or("未知书名")
        .to_string();

    let author = dir_data["data"]["novel"]["author"]
        .as_str()
        .or_else(|| dir_data["novel"]["author"].as_str())
        .unwrap_or("未知作者")
        .to_string();

    let intro = dir_data["data"]["novel"]["abstract"]
        .as_str()
        .or_else(|| dir_data["novel"]["abstract"].as_str())
        .map(|s| s.to_string());

    let cover_url = dir_data["data"]["novel"]["thumb_url"]
        .as_str()
        .or_else(|| dir_data["novel"]["thumb_url"].as_str())
        .map(|s| s.to_string());

    let tags: Vec<String> = dir_data["data"]["novel"]["category"]
        .as_str()
        .map(|s| vec![s.to_string()])
        .unwrap_or_default();

    let finished = dir_data["data"]["novel"]["creation_status"]
        .as_str()
        .map(|s| s == "2");

    let chapter_count = dir_data["data"]["novel"]["total_chapter"]
        .as_u64()
        .map(|n| n as u32);

    // Fallback: try page HTML if API data is insufficient
    if title == "未知书名" {
        match fetch_book_info_from_page(client, novel_id) {
            Ok((t, a, i, c, g, f, cc)) => {
                return (t, a, i, c, g, f, cc);
            }
            Err(_) => {}
        }
    }

    (title, author, intro, cover_url, tags, finished, chapter_count)
}

fn fetch_book_info_from_page(
    client: &Client,
    novel_id: &str,
) -> Result<(
    String,
    String,
    Option<String>,
    Option<String>,
    Vec<String>,
    Option<bool>,
    Option<u32>,
)> {
    let url = format!("https://fanqienovel.com/page/{novel_id}");
    let resp = client
        .get(&url)
        .header(USER_AGENT, DEFAULT_UA)
        .send()
        .context("获取番茄小说页面失败")?;

    if !resp.status().is_success() {
        bail!("番茄小说页面返回 {}", resp.status());
    }

    let html = resp.text().context("读取番茄小说页面失败")?;

    // Parse SSR data from __NEXT_DATA__ or inline JSON
    let next_data_re =
        Regex::new(r#"<script[^>]*id="__NEXT_DATA__"[^>]*>(.*?)</script>"#).ok();
    let ssr_data = next_data_re
        .and_then(|re| re.captures(&html))
        .and_then(|cap| cap.get(1))
        .and_then(|m| serde_json::from_str::<Value>(m.as_str()).ok());

    let novel = ssr_data
        .as_ref()
        .and_then(|d| d["props"]["pageProps"]["novel"].as_object());

    let title = novel
        .and_then(|n| n["book_name"].as_str())
        .unwrap_or("未知书名")
        .to_string();
    let author = novel
        .and_then(|n| n["author"].as_str())
        .unwrap_or("未知作者")
        .to_string();
    let intro = novel.and_then(|n| n["abstract"].as_str()).map(|s| s.to_string());
    let cover_url = novel.and_then(|n| n["thumb_url"].as_str()).map(|s| s.to_string());
    let finished = novel.and_then(|n| n["creation_status"].as_str()).map(|s| s == "2");
    let chapter_count = novel
        .and_then(|n| n["total_chapter"].as_u64())
        .map(|n| n as u32);
    let tags: Vec<String> = novel
        .and_then(|n| n["category"].as_str())
        .map(|s| vec![s.to_string()])
        .unwrap_or_default();

    Ok((title, author, intro, cover_url, tags, finished, chapter_count))
}

fn extract_chapter_text(value: &Value, _chapter_id: &str, _novel_id: &str) -> Option<String> {
    value["data"]["content"]
        .as_str()
        .or_else(|| value["content"].as_str())
        .map(|s| {
            let re_br = Regex::new(r"<\s*br\s*/?\s*>").unwrap();
            let re_p = Regex::new(r"<\s*/\s*p\s*>").unwrap();
            let text = re_br.replace_all(s, "\n");
            let text = re_p.replace_all(&text, "\n");
            let re_tag = Regex::new(r"<[^>]*>").unwrap();
            let text = re_tag.replace_all(&text, "");
            text.trim().to_string()
        })
}

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
