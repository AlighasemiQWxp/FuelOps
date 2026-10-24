use argon2::{
    password_hash::{PasswordHasher, SaltString},
    Argon2,
};
use async_trait::async_trait;
use axum::{
    extract::{Path, Query, State},
    routing::get,
    Extension, Json, Router,
};
use fuel_core::{
    bson::{self, doc, Document},
    events::DomainEvent,
    *,
};
use fuel_permission::User;
use futures_util::TryStreamExt;
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
struct EmployeeInput {
    name: String,
    email: String,
    password: Option<String>,
    station_ids: Vec<String>,
    role_ids: Vec<String>,
    active: bool,
    version: Option<i64>,
}
#[derive(Serialize)]
struct EmployeeChanged {
    employee_id: String,
}
impl DomainEvent for EmployeeChanged {
    const NAME: &'static str = "EmployeePermissionChanged";
}
fn public(mut row: Document) -> Document {
    if let Ok(data) = row.get_document_mut("data") {
        data.remove("password_hash");
    }
    row.remove("_id");
    row
}
pub struct EmployeeModule;
#[async_trait]
impl Module for EmployeeModule {
    fn name(&self) -> &'static str {
        "EmployeeModule"
    }
    fn dependencies(&self) -> &'static [&'static str] {
        &["PermissionModule"]
    }
    fn routes(&self) -> Router<AppState> {
        Router::new()
            .route("/employees", get(list).post(create))
            .route("/employees/{id}", axum::routing::put(update))
    }
    async fn initialize(&self, state: &AppState) -> ApiResult<()> {
        state.record_indexes("users").await?;
        state
            .index("users", doc! {"org_id":1,"data.email":1}, true)
            .await
    }
}
async fn list(
    State(state): State<AppState>,
    Extension(actor): Extension<Actor>,
    Query(query): Query<PageQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    actor.require("employee.view")?;
    let filter = doc! {"org_id":&actor.org_id,"data.station_ids":&actor.station_id};
    let col = state.db.collection::<Document>("users");
    let total = col.count_documents(filter.clone()).await?;
    let items: Vec<Document> = col
        .find(filter)
        .projection(doc! {"data.password_hash":0,"_id":0})
        .sort(doc! {"created_at":-1})
        .skip(query.skip())
        .limit(query.limit())
        .await?
        .try_collect()
        .await?;
    Ok(Json(serde_json::json!({"items":items,"total":total})))
}
async fn password(value: String) -> ApiResult<String> {
    require(
        value.len() >= 12 && value.len() <= 128,
        "Password must contain 12–128 characters",
    )?;
    tokio::task::spawn_blocking(move || {
        Argon2::default()
            .hash_password(
                value.as_bytes(),
                &SaltString::generate(&mut rand::rngs::OsRng),
            )
            .map(|v| v.to_string())
            .map_err(|_| ApiError::validation("Password hashing failed"))
    })
    .await
    .map_err(|_| ApiError::validation("Password hashing failed"))?
}
fn validate(input: &EmployeeInput) -> ApiResult<()> {
    require(
        !input.name.trim().is_empty()
            && input.name.len() <= 120
            && input.email.contains('@')
            && input.email.len() <= 254,
        "A valid name and email are required",
    )
}
async fn create(
    State(state): State<AppState>,
    Extension(actor): Extension<Actor>,
    Json(input): Json<EmployeeInput>,
) -> ApiResult<Json<Document>> {
    actor.require("employee.create")?;
    actor.require("permission.manage")?;
    validate(&input)?;
    let password_hash = password(input.password.clone().unwrap_or_default()).await?;
    let mut tx = state.begin(&actor).await?;
    fuel_permission::validate_assignments(&mut tx, &input.role_ids, &input.station_ids).await?;
    let record = Record::new(
        &actor,
        User {
            name: input.name,
            email: input.email.trim().to_lowercase(),
            password_hash,
            active: input.active,
            station_ids: input.station_ids,
            role_ids: input.role_ids,
        },
    );
    tx.insert("users", &record).await?;
    tx.emit(
        &record.id,
        EmployeeChanged {
            employee_id: record.id.clone(),
        },
    )
    .await?;
    tx.commit().await?;
    Ok(Json(public(bson::to_document(&record)?)))
}
async fn update(
    State(state): State<AppState>,
    Extension(actor): Extension<Actor>,
    Path(id): Path<String>,
    Json(input): Json<EmployeeInput>,
) -> ApiResult<Json<Document>> {
    actor.require("employee.update")?;
    actor.require("permission.manage")?;
    validate(&input)?;
    require(
        id != actor.user_id,
        "Use a second administrator to change your own access",
    )?;
    let new_password = if let Some(value) = input.password.clone().filter(|p| !p.is_empty()) {
        Some(password(value).await?)
    } else {
        None
    };
    let mut tx = state.begin(&actor).await?;
    fuel_permission::validate_assignments(&mut tx, &input.role_ids, &input.station_ids).await?;
    let raw = tx
        .db()
        .collection::<Document>("users")
        .find_one(doc! {"org_id":&actor.org_id,"id":&id,"data.station_ids":&actor.station_id})
        .session(&mut tx.session)
        .await?
        .ok_or_else(ApiError::missing)?;
    let mut record: Record<User> = bson::from_document(raw)?;
    fuel_permission::validate_assignments(&mut tx, &record.data.role_ids, &record.data.station_ids)
        .await?;
    require(
        record.station_id == actor.station_id,
        "Edit this account from its home station",
    )?;
    if !input.active {
        actor.require("employee.delete")?;
    }
    record.data = User {
        name: input.name,
        email: input.email.trim().to_lowercase(),
        password_hash: new_password.unwrap_or(record.data.password_hash),
        active: input.active,
        station_ids: input.station_ids,
        role_ids: input.role_ids,
    };
    tx.replace(
        "users",
        &mut record,
        input.version.ok_or_else(ApiError::conflict)?,
    )
    .await?;
    tx.db()
        .collection::<Document>("refresh_sessions")
        .update_many(
            doc! {"org_id":&actor.org_id,"data.user_id":&id},
            doc! {"$set":{"data.revoked":true}},
        )
        .session(&mut tx.session)
        .await?;
    tx.emit(
        &id,
        EmployeeChanged {
            employee_id: id.clone(),
        },
    )
    .await?;
    tx.commit().await?;
    Ok(Json(public(bson::to_document(&record)?)))
}
