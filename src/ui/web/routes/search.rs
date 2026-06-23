use axum::Json;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use serde::Deserialize;
use serde_json::{Value, json};

#[cfg(feature = "official-api")]
use tomato_novel_official_api::SearchClient;

use crate::ui::web::state::AppState;

#[derive(Debug, Deserialize)]
pub(crate) struct SearchQuery {
    pub(crate) q: String,
    /// 逗号分隔的平台 ID,为空则搜索所有已接入平台
    #[serde(default)]
    pub(crate) platform: String,
}

pub(crate) async fn api_search(
    State(state): State<AppState>,
    Query(q): Query<SearchQuery>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let keyword = q.q.trim().to_string();
    if keyword.is_empty() {
        return Ok(Json(json!({"items": []})));
    }

    let only: Vec<String> = if q.platform.is_empty() {
        Vec::new()
    } else {
        q.platform
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    };

    // 并发限制
    #[cfg(feature = "official-api")]
    let _permit = state
        .api_semaphore
        .acquire()
        .await
        .map_err(|_| api_error(StatusCode::SERVICE_UNAVAILABLE, "上游 API 并发限制已关闭"))?;

    // 多平台搜索聚合
    let registry = state.platform_registry.clone();
    let kw = keyword.clone();
    let only_clone = only.clone();
    let results = tokio::task::spawn_blocking(move || registry.search_across(&kw, &only_clone))
        .await
        .map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "搜索任务执行失败"))?;

    // 如果多平台均无结果,且启用了 official-api,回退到番茄官方搜索
    #[cfg(feature = "official-api")]
    if results.is_empty() && only.is_empty() {
        let kw2 = keyword.clone();
        let fallback = tokio::task::spawn_blocking(move || {
            let client = SearchClient::new()?;
            client.search_books(&kw2)
        })
        .await
        .map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "搜索任务执行失败"))?;

        match fallback {
            Ok(resp) => {
                let items: Vec<Value> = resp
                    .books
                    .into_iter()
                    .map(|b| {
                        json!({
                            "book_id": format!("fanqie:{}", b.book_id),
                            "platform_id": "fanqie",
                            "platform_name": "番茄小说",
                            "title": b.title,
                            "author": b.author,
                        })
                    })
                    .collect();
                return Ok(Json(json!({"items": items})));
            }
            Err(_) => {}
        }
    }

    let items: Vec<Value> = results
        .into_iter()
        .map(|r| {
            json!({
                "book_id": r.id.to_string(),
                "platform_id": r.id.platform,
                "platform_name": r.platform_name,
                "title": r.title,
                "author": r.author,
                "intro": r.intro,
                "cover_url": r.cover_url,
                "chapter_count": r.chapter_count,
                "finished": r.finished,
            })
        })
        .collect();

    Ok(Json(json!({"items": items})))
}

fn api_error(status: StatusCode, message: impl Into<String>) -> (StatusCode, Json<Value>) {
    (status, Json(json!({ "error": message.into() })))
}
