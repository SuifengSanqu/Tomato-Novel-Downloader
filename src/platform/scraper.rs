//! 通用 web scraping 工具。
//!
//! 用于没有官方 API 的平台(如七猫),通过 HTTP 请求 + HTML 解析获取内容。
//! 依赖 reqwest 和 regex。

use anyhow::{Context, Result};
use regex::Regex;
use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, ACCEPT_LANGUAGE, USER_AGENT};
use std::time::Duration;

const DEFAULT_USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36";

/// 通用 HTTP 爬取客户端,带 UA 伪装和超时控制。
pub struct ScraperClient {
    client: Client,
    user_agent: String,
}

impl ScraperClient {
    pub fn new() -> Result<Self> {
        let mut headers = HeaderMap::new();
        headers.insert(USER_AGENT, HeaderValue::from_static(DEFAULT_USER_AGENT));
        headers.insert(ACCEPT, HeaderValue::from_static("text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8"));
        headers.insert(ACCEPT_LANGUAGE, HeaderValue::from_static("zh-CN,zh;q=0.9,en;q=0.8"));

        let client = Client::builder()
            .default_headers(headers)
            .timeout(Duration::from_secs(15))
            .user_agent(DEFAULT_USER_AGENT)
            .build()
            .context("创建 HTTP 客户端失败")?;

        Ok(Self {
            client,
            user_agent: DEFAULT_USER_AGENT.to_string(),
        })
    }

    pub fn get(&self, url: &str) -> Result<String> {
        let resp = self
            .client
            .get(url)
            .header(USER_AGENT, &self.user_agent)
            .send()
            .with_context(|| format!("HTTP GET {url} 失败"))?;

        let status = resp.status();
        if !status.is_success() {
            anyhow::bail!("HTTP GET {url} 返回 {status}");
        }

        resp.text()
            .with_context(|| format!("读取 {url} 响应体失败"))
    }

    pub fn get_json(&self, url: &str) -> Result<serde_json::Value> {
        let resp = self
            .client
            .get(url)
            .header(USER_AGENT, &self.user_agent)
            .header("Accept", "application/json, text/plain, */*")
            .send()
            .with_context(|| format!("HTTP GET JSON {url} 失败"))?;

        let status = resp.status();
        if !status.is_success() {
            anyhow::bail!("HTTP GET JSON {url} 返回 {status}");
        }

        resp.json()
            .with_context(|| format!("解析 {url} JSON 失败"))
    }

    pub fn post_json(&self, url: &str, body: &serde_json::Value) -> Result<serde_json::Value> {
        let resp = self
            .client
            .post(url)
            .header(USER_AGENT, &self.user_agent)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/plain, */*")
            .json(body)
            .send()
            .with_context(|| format!("HTTP POST {url} 失败"))?;

        let status = resp.status();
        if !status.is_success() {
            anyhow::bail!("HTTP POST {url} 返回 {status}");
        }

        resp.json()
            .with_context(|| format!("解析 {url} JSON 失败"))
    }
}

/// 从 HTML 中提取 <script id="__NEXT_DATA__"> 内的 JSON(SSR 页面用)。
pub fn extract_next_data(html: &str) -> Option<serde_json::Value> {
    let re = Regex::new(r#"<script[^>]*id="__NEXT_DATA__"[^>]*type="application/json"[^>]*>(.*?)</script>"#)
        .ok()?;
    let caps = re.captures(html)?;
    let json_str = caps.get(1)?.as_str();
    serde_json::from_str(json_str).ok()
}

/// 从 HTML 中提取 window.__NUXT__ 或 window.__INITIAL_STATE__ 等全局 JSON 对象。
pub fn extract_initial_state(html: &str, var_name: &str) -> Option<serde_json::Value> {
    let pattern = format!(r#"window\.__{var_name}__\s*=\s*(\{{.*?\}})\s*;"#);
    let re = Regex::new(&pattern).ok()?;
    let caps = re.captures(html)?;
    let json_str = caps.get(1)?.as_str();
    serde_json::from_str(json_str).ok()
}

/// 从 HTML meta 标签提取元数据(title, description等)。
pub fn extract_meta(html: &str) -> std::collections::HashMap<String, String> {
    let mut meta = std::collections::HashMap::new();

    let title_re = Regex::new(r"<title[^>]*>(.*?)</title>").ok();
    if let Some(re) = &title_re {
        if let Some(cap) = re.captures(html) {
            meta.insert("title".to_string(), cap.get(1).unwrap().as_str().to_string());
        }
    }

    let og_re = Regex::new(r#"<meta[^>]*property="og:(\w+)"[^>]*content="([^"]*)"[^>]*>"#).ok();
    if let Some(re) = &og_re {
        for cap in re.captures_iter(html) {
            let key = format!("og:{}", &cap[1]);
            meta.insert(key, cap[2].to_string());
        }
    }

    let desc_re = Regex::new(r#"<meta[^>]*name="description"[^>]*content="([^"]*)"[^>]*>"#).ok();
    if let Some(re) = &desc_re {
        if let Some(cap) = re.captures(html) {
            meta.insert("description".to_string(), cap[1].to_string());
        }
    }

    meta
}
