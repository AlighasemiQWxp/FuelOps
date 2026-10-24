use async_trait::async_trait;
use axum::{
    extract::{Path, Query, State},
    routing::{get, post},
    Extension, Json, Router,
};
use fuel_core::{bson::doc, events::DomainEvent, *};
use fuel_inventory::InventoryManager;
use fuel_phases::{BasePhasedController, PhaseActions};
use fuel_station::StationManager;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DeliveryPhase {
    Registered,
    AwaitingMeasurements,
    Measuring,
    Reviewing,
    Approved,
    Completed,
    Cancelled,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Delivery {
    pub document_number: String,
    pub supplier: String,
    pub tank_id: String,
    pub fuel_type_id: String,
    pub declared_ml: i64,
    pub before_ml: Option<i64>,
    pub after_ml: Option<i64>,
    pub measured_ml: Option<i64>,
    pub difference_ml: Option<i64>,
    pub tolerance_ml: i64,
    pub source: String,
    pub status: DeliveryPhase,
    pub note: String,
    pub approved_by: Option<String>,
}
#[derive(Deserialize)]
struct Create {
    document_number: String,
    supplier: String,
    tank_id: String,
    fuel_type_id: String,
    declared_ml: i64,
    source: String,
}
#[derive(Deserialize)]
struct Transition {
    version: i64,
    to: DeliveryPhase,
    before_ml: Option<i64>,
    after_ml: Option<i64>,
    note: Option<String>,
}
#[derive(Serialize)]
pub struct FuelDeliveryChanged {
    pub phase: DeliveryPhase,
    pub tank_id: String,
    pub quantity_ml: i64,
}
impl DomainEvent for FuelDeliveryChanged {
    const NAME: &'static str = "FuelDeliveryChanged";
}
struct Actions;
#[async_trait]
impl PhaseActions<DeliveryPhase> for Actions {}
async fn permits(from: DeliveryPhase, to: DeliveryPhase) -> ApiResult<()> {
    use DeliveryPhase::*;
    let controller = BasePhasedController::new(
        from,
        [
            (Registered, vec![AwaitingMeasurements, Cancelled]),
            (AwaitingMeasurements, vec![Measuring, Cancelled]),
            (Measuring, vec![Reviewing]),
            (Reviewing, vec![Measuring, Approved]),
            (Approved, vec![Completed]),
            (Completed, vec![]),
            (Cancelled, vec![]),
        ],
        Actions,
    )
    .map_err(|e| ApiError::validation(e.to_string()))?;
    controller
        .initialize()
        .await
        .map_err(|e| ApiError::validation(e.to_string()))?;
    controller
        .transition_to(to)
        .await
        .map_err(|e| ApiError::validation(e.to_string()))
}
pub struct DeliveryModule;
#[async_trait]
impl Module for DeliveryModule {
    fn name(&self) -> &'static str {
        "DeliveryModule"
    }
    fn dependencies(&self) -> &'static [&'static str] {
        &["StationModule", "InventoryModule"]
    }
    fn routes(&self) -> Router<AppState> {
        Router::new()
            .route("/deliveries", get(list).post(create))
            .route("/deliveries/{id}/transition", post(transition))
    }
    async fn initialize(&self, state: &AppState) -> ApiResult<()> {
        state.record_indexes("deliveries").await?;
        state
            .index(
                "deliveries",
                doc! {"org_id":1,"station_id":1,"data.document_number":1},
                true,
            )
            .await
    }
}
async fn list(
    State(state): State<AppState>,
    Extension(actor): Extension<Actor>,
    Query(query): Query<PageQuery>,
) -> ApiResult<Json<Page<Record<Delivery>>>> {
    actor.require("delivery.view")?;
    Ok(Json(state.list(&actor, "deliveries", query).await?))
}
async fn create(
    State(state): State<AppState>,
    Extension(actor): Extension<Actor>,
    Json(input): Json<Create>,
) -> ApiResult<Json<Record<Delivery>>> {
    actor.require("delivery.create")?;
    require(
        !input.document_number.trim().is_empty()
            && input.document_number.len() <= 100
            && input.supplier.len() <= 160
            && input.declared_ml > 0
            && ["manual", "simulated"].contains(&input.source.as_str()),
        "Invalid delivery document or source",
    )?;
    let mut tx = state.begin(&actor).await?;
    let tank = StationManager::tank(&mut tx, &input.tank_id).await?;
    let station = StationManager::configuration(&mut tx).await?;
    require(
        tank.fuel_type_id == input.fuel_type_id && input.declared_ml <= tank.capacity_ml,
        "Fuel type or quantity does not match tank",
    )?;
    let record = Record::new(
        &actor,
        Delivery {
            document_number: input.document_number,
            supplier: input.supplier,
            tank_id: input.tank_id,
            fuel_type_id: input.fuel_type_id,
            declared_ml: input.declared_ml,
            before_ml: None,
            after_ml: None,
            measured_ml: None,
            difference_ml: None,
            tolerance_ml: station.data.delivery_tolerance_ml,
            source: input.source,
            status: DeliveryPhase::Registered,
            note: String::new(),
            approved_by: None,
        },
    );
    tx.insert("deliveries", &record).await?;
    tx.emit(
        &record.id,
        FuelDeliveryChanged {
            phase: record.data.status,
            tank_id: record.data.tank_id.clone(),
            quantity_ml: 0,
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
) -> ApiResult<Json<Record<Delivery>>> {
    use DeliveryPhase::*;
    if matches!(input.to, Approved | Completed) {
        actor.require("delivery.approve")?;
    } else {
        actor.require("delivery.create")?;
    }
    let mut tx = state.begin(&actor).await?;
    let mut record: Record<Delivery> = tx.get("deliveries", &id).await?;
    permits(record.data.status, input.to).await?;
    if let Some(note) = input.note {
        require(note.len() <= 4000, "Note is too long")?;
        record.data.note = note;
    }
    if input.to == Reviewing {
        let before = input
            .before_ml
            .ok_or_else(|| ApiError::validation("Before-delivery volume is required"))?;
        let after = input
            .after_ml
            .ok_or_else(|| ApiError::validation("After-delivery volume is required"))?;
        let tank = StationManager::tank(&mut tx, &record.data.tank_id).await?;
        require(
            before >= 0 && after >= before && after <= tank.capacity_ml,
            "Invalid before/after delivery measurements",
        )?;
        record.data.before_ml = Some(before);
        record.data.after_ml = Some(after);
        record.data.measured_ml = Some(after - before);
        record.data.difference_ml = Some(after - before - record.data.declared_ml);
    }
    if input.to == Approved {
        let difference = record
            .data
            .difference_ml
            .ok_or_else(|| ApiError::validation("Measurements are required"))?;
        if difference.unsigned_abs() > record.data.tolerance_ml as u64 {
            require(
                !record.data.note.trim().is_empty(),
                "Explain the quantity discrepancy before approval",
            )?;
        }
        record.data.approved_by = Some(actor.user_id.clone());
    }
    if input.to == Completed {
        let quantity = record
            .data
            .measured_ml
            .ok_or_else(|| ApiError::validation("Measurements are required"))?;
        InventoryManager::change(
            &mut tx,
            &record.data.tank_id,
            quantity,
            &id,
            "delivery",
            &record.data.source,
        )
        .await?;
    }
    record.data.status = input.to;
    tx.replace("deliveries", &mut record, input.version).await?;
    tx.emit(
        &id,
        FuelDeliveryChanged {
            phase: input.to,
            tank_id: record.data.tank_id.clone(),
            quantity_ml: record.data.measured_ml.unwrap_or(0),
        },
    )
    .await?;
    tx.commit().await?;
    Ok(Json(record))
}
