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
use futures_util::TryStreamExt;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const PERMISSIONS: &[&str] = &[
    "station.view",
    "station.manage",
    "employee.view",
    "employee.create",
    "employee.update",
    "employee.delete",
    "permission.manage",
    "shift.view",
    "shift.create",
    "shift.open",
    "shift.close",
    "shift.approve",
    "inventory.view",
    "inventory.manage",
    "delivery.view",
    "delivery.create",
    "delivery.approve",
    "reconciliation.view",
    "reconciliation.manage",
    "reconciliation.approve",
    "alert.view",
    "alert.manage",
    "dashboard.view",
    "report.export",
    "audit.view",
];
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Role {
    pub name: String,
    pub permissions: HashSet<String>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct User {
    pub name: String,
    pub email: String,
    pub password_hash: String,
    pub active: bool,
    pub station_ids: Vec<String>,
    pub role_ids: Vec<String>,
}
#[derive(Serialize)]
pub struct EmployeePermissionChanged {
    pub role_id: String,
}
impl DomainEvent for EmployeePermissionChanged {
    const NAME: &'static str = "EmployeePermissionChanged";
}

pub async fn resolve(
    state: &AppState,
    org: &str,
    user_id: &str,
    station: Option<&str>,
) -> ApiResult<Actor> {
    let raw = state
        .db
        .collection::<Document>("users")
        .find_one(doc! {"org_id":org,"id":user_id,"data.active":true})
        .await?
        .ok_or_else(ApiError::unauthorized)?;
    let user: Record<User> = bson::from_document(raw)?;
    let station = station
        .or_else(|| user.data.station_ids.first().map(String::as_str))
        .ok_or_else(ApiError::forbidden)?;
    if !user.data.station_ids.iter().any(|s| s == station) {
        return Err(ApiError::forbidden());
    }
    let roles: Vec<Document> = state
        .db
        .collection::<Document>("roles")
        .find(doc! {"org_id":org,"station_id":station,"id":{"$in": &user.data.role_ids}})
        .await?
        .try_collect()
        .await?;
    let mut permissions = HashSet::new();
    for raw in roles {
        let role: Record<Role> = bson::from_document(raw)?;
        permissions.extend(role.data.permissions);
    }
    Ok(Actor {
        user_id: user.id,
        org_id: org.into(),
        station_id: station.into(),
        station_ids: user.data.station_ids,
        permissions,
        name: user.data.name,
    })
}

pub async fn validate_assignments(
    tx: &mut Transaction,
    roles: &[String],
    stations: &[String],
) -> ApiResult<()> {
    require(
        !stations.is_empty() && stations.iter().all(|s| tx.actor.station_ids.contains(s)),
        "Invalid station assignments",
    )?;
    for id in roles {
        let raw = tx
            .db()
            .collection::<Document>("roles")
            .find_one(doc! {"org_id":&tx.actor.org_id,"id":id,"station_id":{"$in": stations}})
            .session(&mut tx.session)
            .await?
            .ok_or_else(ApiError::forbidden)?;
        let role: Record<Role> = bson::from_document(raw)?;
        let owner = tx
            .db()
            .collection::<Document>("users")
            .find_one(doc! {"org_id":&tx.actor.org_id,"id":&tx.actor.user_id,"data.active":true})
            .session(&mut tx.session)
            .await?
            .ok_or_else(ApiError::unauthorized)?;
        let owner: Record<User> = bson::from_document(owner)?;
        let mut cursor = tx.db().collection::<Document>("roles").find(doc! {"org_id":&tx.actor.org_id,"station_id":&role.station_id,"id":{"$in":&owner.data.role_ids}}).session(&mut tx.session).await?;
        let granted: Vec<Document> = cursor.stream(&mut tx.session).try_collect().await?;
        let mut permissions = HashSet::new();
        for grant in granted {
            let grant: Record<Role> = bson::from_document(grant)?;
            permissions.extend(grant.data.permissions);
        }
        require(
            permissions.contains("permission.manage")
                && role.data.permissions.is_subset(&permissions),
            "Cannot grant permissions you do not hold at the assigned station",
        )?;
    }
    Ok(())
}
pub struct PermissionModule;
#[async_trait]
impl Module for PermissionModule {
    fn name(&self) -> &'static str {
        "PermissionModule"
    }
    fn routes(&self) -> Router<AppState> {
        Router::new()
            .route("/roles", get(list).post(create))
            .route("/roles/{id}", axum::routing::put(update))
            .route("/permissions", get(catalog))
    }
    async fn initialize(&self, state: &AppState) -> ApiResult<()> {
        state.record_indexes("roles").await?;
        state
            .index(
                "roles",
                doc! {"org_id":1,"station_id":1,"data.name":1},
                true,
            )
            .await
    }
}
async fn catalog(Extension(actor): Extension<Actor>) -> ApiResult<Json<serde_json::Value>> {
    actor.require("permission.manage")?;
    Ok(Json(serde_json::json!({"permissions":PERMISSIONS})))
}
async fn list(
    State(state): State<AppState>,
    Extension(actor): Extension<Actor>,
    Query(query): Query<PageQuery>,
) -> ApiResult<Json<Page<Record<Role>>>> {
    actor.require("permission.manage")?;
    Ok(Json(state.list(&actor, "roles", query).await?))
}
fn validate(actor: &Actor, role: &Role) -> ApiResult<()> {
    actor.require("permission.manage")?;
    require(
        !role.name.trim().is_empty() && role.name.len() <= 120,
        "Role name is required",
    )?;
    require(
        role.permissions
            .iter()
            .all(|p| PERMISSIONS.contains(&p.as_str()))
            && role.permissions.is_subset(&actor.permissions),
        "Invalid or excessive permissions",
    )
}
async fn create(
    State(state): State<AppState>,
    Extension(actor): Extension<Actor>,
    Json(role): Json<Role>,
) -> ApiResult<Json<Record<Role>>> {
    validate(&actor, &role)?;
    let record = Record::new(&actor, role);
    let mut tx = state.begin(&actor).await?;
    tx.insert("roles", &record).await?;
    tx.emit(
        &record.id,
        EmployeePermissionChanged {
            role_id: record.id.clone(),
        },
    )
    .await?;
    tx.commit().await?;
    Ok(Json(record))
}
#[derive(Deserialize)]
struct RoleUpdate {
    version: i64,
    name: String,
    permissions: HashSet<String>,
}
async fn update(
    State(state): State<AppState>,
    Extension(actor): Extension<Actor>,
    Path(id): Path<String>,
    Json(input): Json<RoleUpdate>,
) -> ApiResult<Json<Record<Role>>> {
    let role = Role {
        name: input.name,
        permissions: input.permissions,
    };
    validate(&actor, &role)?;
    let mut tx = state.begin(&actor).await?;
    let mut record: Record<Role> = tx.get("roles", &id).await?;
    let own = tx
        .db()
        .collection::<Document>("users")
        .find_one(doc! {"org_id":&actor.org_id,"id":&actor.user_id,"data.role_ids":&id})
        .session(&mut tx.session)
        .await?;
    require(
        own.is_none(),
        "Use a second authorized administrator to change your own role",
    )?;
    require(
        record.data.permissions.is_subset(&actor.permissions),
        "Cannot edit a role above your permissions",
    )?;
    record.data = role;
    tx.replace("roles", &mut record, input.version).await?;
    tx.emit(
        &id,
        EmployeePermissionChanged {
            role_id: id.clone(),
        },
    )
    .await?;
    tx.commit().await?;
    Ok(Json(record))
}
