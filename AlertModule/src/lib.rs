use async_trait::async_trait;
use axum::{
    extract::{Path, Query, State},
    routing::{get, post},
    Extension, Json, Router,
};
use fuel_core::{
    bson::{self, doc, Document},
    events::{AppEvent, DomainEvent},
    *,
};
use fuel_station::StationManager;
use futures_util::TryStreamExt;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Alert {
    pub kind: String,
    pub severity: String,
    pub title: String,
    pub description: String,
    pub entity_id: String,
    pub assigned_to: Option<String>,
    pub status: String,
    pub resolution: String,
    pub source: String,
    pub history: Vec<History>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct History {
    pub at: i64,
    pub actor: String,
    pub status: String,
    pub note: String,
}
#[derive(Deserialize)]
struct Create {
    kind: String,
    severity: String,
    title: String,
    description: String,
    entity_id: String,
    assigned_to: Option<String>,
}
#[derive(Deserialize)]
struct Transition {
    version: i64,
    to: String,
    note: String,
    assigned_to: Option<String>,
}
#[derive(Serialize)]
struct AlertChanged {
    status: String,
}
impl DomainEvent for AlertChanged {
    const NAME: &'static str = "AlertChanged";
}
pub struct AlertModule;
#[async_trait]
impl Module for AlertModule {
    fn name(&self) -> &'static str {
        "AlertModule"
    }
    fn dependencies(&self) -> &'static [&'static str] {
        &["StationModule"]
    }
    fn routes(&self) -> Router<AppState> {
        Router::new()
            .route("/alerts", get(list).post(create))
            .route("/alerts/{id}/transition", post(transition))
    }
    async fn initialize(&self, state: &AppState) -> ApiResult<()> {
        state.record_indexes("alerts").await?;
        state
            .index("outbox", doc! {"alert_processed":1,"occurred_at":1}, false)
            .await
    }
}
async fn list(
    State(state): State<AppState>,
    Extension(actor): Extension<Actor>,
    Query(query): Query<PageQuery>,
) -> ApiResult<Json<Page<Record<Alert>>>> {
    actor.require("alert.view")?;
    Ok(Json(state.list(&actor, "alerts", query).await?))
}
async fn assignee(tx: &mut Transaction, id: Option<&str>) -> ApiResult<()> {
    if let Some(id) = id.filter(|id| !id.is_empty()) {
        let user=tx.db().collection::<Document>("users").find_one(doc! {"org_id":&tx.actor.org_id,"id":id,"data.active":true,"data.station_ids":&tx.actor.station_id}).session(&mut tx.session).await?;
        require(user.is_some(), "Assignee is not an active station employee")?;
    }
    Ok(())
}
async fn create(
    State(state): State<AppState>,
    Extension(actor): Extension<Actor>,
    Json(input): Json<Create>,
) -> ApiResult<Json<Record<Alert>>> {
    actor.require("alert.manage")?;
    require(
        [
            "nozzle",
            "reading",
            "inventory",
            "overdue_shift",
            "handover",
            "discrepancy",
            "delivery",
            "other",
        ]
        .contains(&input.kind.as_str())
            && ["low", "medium", "high"].contains(&input.severity.as_str())
            && !input.title.trim().is_empty()
            && input.title.len() <= 160
            && input.description.len() <= 4000,
        "Invalid operational issue",
    )?;
    let mut tx = state.begin(&actor).await?;
    assignee(&mut tx, input.assigned_to.as_deref()).await?;
    let record = Record::new(
        &actor,
        Alert {
            kind: input.kind,
            severity: input.severity,
            title: input.title,
            description: input.description,
            entity_id: input.entity_id,
            assigned_to: input.assigned_to,
            status: "Open".into(),
            resolution: String::new(),
            source: "manual".into(),
            history: vec![History {
                at: now(),
                actor: actor.name.clone(),
                status: "Open".into(),
                note: "ثبت رویداد".into(),
            }],
        },
    );
    tx.insert("alerts", &record).await?;
    tx.emit(
        &record.id,
        AlertChanged {
            status: "Open".into(),
        },
    )
    .await?;
    tx.commit().await?;
    Ok(Json(record))
}
async fn transition(
    State(state): State<AppState>,
    Extension(actor): Extension<Actor>,
    Path(id): Path<String>,
    Json(input): Json<Transition>,
) -> ApiResult<Json<Record<Alert>>> {
    actor.require("alert.manage")?;
    require(input.note.len() <= 4000, "Note is too long")?;
    let mut tx = state.begin(&actor).await?;
    let mut record: Record<Alert> = tx.get("alerts", &id).await?;
    let allowed = matches!(
        (record.data.status.as_str(), input.to.as_str()),
        ("Open", "Acknowledged") | ("Acknowledged", "Resolved") | ("Resolved", "Open")
    );
    require(allowed, "Invalid issue transition")?;
    if input.to == "Resolved" {
        require(
            !input.note.trim().is_empty(),
            "Resolution notes are required",
        )?;
        record.data.resolution = input.note.clone();
    }
    assignee(&mut tx, input.assigned_to.as_deref()).await?;
    record.data.assigned_to = input.assigned_to;
    record.data.status = input.to.clone();
    record.data.history.push(History {
        at: now(),
        actor: actor.name.clone(),
        status: input.to.clone(),
        note: input.note,
    });
    tx.replace("alerts", &mut record, input.version).await?;
    tx.emit(&id, AlertChanged { status: input.to }).await?;
    tx.commit().await?;
    Ok(Json(record))
}
fn system(org: String, station: String) -> Actor {
    Actor {
        user_id: "system".into(),
        org_id: org,
        station_id: station.clone(),
        station_ids: vec![station],
        permissions: Default::default(),
        name: "سامانه".into(),
    }
}
async fn issue(
    tx: &mut Transaction,
    id: &str,
    kind: &str,
    title: &str,
    description: &str,
    entity: &str,
) -> ApiResult<()> {
    if !tx.find("alerts", doc! {"id":id}).await?.is_empty() {
        return Ok(());
    }
    let mut record = Record::new(
        &tx.actor,
        Alert {
            kind: kind.into(),
            severity: "medium".into(),
            title: title.into(),
            description: description.into(),
            entity_id: entity.into(),
            assigned_to: None,
            status: "Open".into(),
            resolution: String::new(),
            source: "calculated".into(),
            history: vec![History {
                at: now(),
                actor: "سامانه".into(),
                status: "Open".into(),
                note: "بر پایه اطلاعات ثبت‌شده؛ پایش ایمنی صنعتی نیست".into(),
            }],
        },
    );
    record.id = id.into();
    tx.insert("alerts", &record).await
}
pub async fn process_events(state: &AppState) -> ApiResult<()> {
    let events: Vec<Document> = state
        .db
        .collection::<Document>("outbox")
        .find(doc! {"alert_processed":{"$ne":true}})
        .sort(doc! {"occurred_at":1})
        .limit(100)
        .await?
        .try_collect()
        .await?;
    for raw in events {
        let event: AppEvent = bson::from_document(raw)?;
        let actor = system(event.org_id, event.station_id);
        let mut tx = state.begin(&actor).await?;
        if event.name == "DiscrepancyDetected" {
            issue(
                &mut tx,
                &event.id,
                "discrepancy",
                "اختلاف تسویه",
                event
                    .payload
                    .get_str("description")
                    .unwrap_or("نیازمند بررسی"),
                &event.entity_id,
            )
            .await?;
        }
        if event.name == "InventoryUpdated" {
            if let Ok(tank) = StationManager::tank(&mut tx, &event.entity_id).await {
                if event.payload.get_i64("volume_ml").unwrap_or(i64::MAX) < tank.low_threshold_ml {
                    issue(
                        &mut tx,
                        &event.id,
                        "inventory",
                        "موجودی ثبت‌شده پایین است",
                        &tank.name,
                        &tank.id,
                    )
                    .await?;
                }
            }
        }
        tx.db()
            .collection::<Document>("outbox")
            .update_one(
                doc! {"id":&event.id},
                doc! {"$set":{"alert_processed":true}},
            )
            .session(&mut tx.session)
            .await?;
        tx.commit().await?;
    }
    let overdue: Vec<Document> = state
        .db
        .collection::<Document>("shifts")
        .find(doc! {"data.status":"Open","data.ends_at":{"$lt":now()}})
        .limit(100)
        .await?
        .try_collect()
        .await?;
    for shift in overdue {
        let actor = system(
            shift.get_str("org_id").unwrap_or_default().into(),
            shift.get_str("station_id").unwrap_or_default().into(),
        );
        let id = shift.get_str("id").unwrap_or_default();
        let mut tx = state.begin(&actor).await?;
        issue(
            &mut tx,
            &format!("overdue:{id}"),
            "overdue_shift",
            "زمان شیفت پایان یافته",
            "شیفت همچنان باز است؛ وضعیت تحویل را بررسی کنید",
            id,
        )
        .await?;
        tx.commit().await?;
    }
    Ok(())
}
