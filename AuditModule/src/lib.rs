use async_trait::async_trait;
use axum::{
    extract::{Path, Query, State},
    routing::get,
    Extension, Json, Router,
};
use fuel_core::{
    bson::{doc, Document},
    *,
};
use futures_util::TryStreamExt;

pub struct AuditModule;
#[async_trait]
impl Module for AuditModule {
    fn name(&self) -> &'static str {
        "AuditModule"
    }
    fn routes(&self) -> Router<AppState> {
        Router::new()
            .route("/audit", get(list))
            .route("/audit/history/{collection}/{id}", get(history))
    }
    async fn initialize(&self, state: &AppState) -> ApiResult<()> {
        state.record_indexes("audit_logs").await
    }
}
async fn list(
    State(state): State<AppState>,
    Extension(actor): Extension<Actor>,
    Query(query): Query<PageQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    actor.require("audit.view")?;
    let col = state.db.collection::<Document>("audit_logs");
    let total = col.count_documents(actor.scope()).await?;
    let items: Vec<Document> = col
        .find(actor.scope())
        .projection(doc! {"_id":0})
        .sort(doc! {"created_at":-1})
        .skip(query.skip())
        .limit(query.limit())
        .await?
        .try_collect()
        .await?;
    Ok(Json(serde_json::json!({"items":items,"total":total})))
}

async fn history(
    State(state): State<AppState>,
    Extension(actor): Extension<Actor>,
    Path((collection, id)): Path<(String, String)>,
) -> ApiResult<Json<Vec<Document>>> {
    actor.require("audit.view")?;
    let mut filter = actor.scope();
    filter.insert("collection", collection);
    filter.insert("entity_id", id);
    Ok(Json(
        state
            .db
            .collection::<Document>("record_history")
            .find(filter)
            .projection(doc! {"_id":0})
            .sort(doc! {"version":-1})
            .limit(100)
            .await?
            .try_collect()
            .await?,
    ))
}
