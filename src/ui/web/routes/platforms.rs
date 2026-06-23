//! 平台管理 API:列出可用平台。

use axum::Json;
use axum::extract::State;
use serde_json::{Value, json};

use crate::ui::web::state::AppState;

pub(crate) async fn api_platforms(State(state): State<AppState>) -> Json<Value> {
    let platforms: Vec<Value> = state
        .platform_registry
        .list_all()
        .into_iter()
        .map(|(id, name, domain)| {
            json!({
                "id": id,
                "name": name,
                "domain": domain,
            })
        })
        .collect();

    Json(json!({ "platforms": platforms }))
}
