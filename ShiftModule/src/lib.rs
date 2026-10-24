use async_trait::async_trait;
use axum::{
    extract::{Path, Query, State},
    routing::{get, post},
    Extension, Json, Router,
};
use fuel_core::{
    bson::{doc, Document},
    events::DomainEvent,
    *,
};
use fuel_phases::{BasePhasedController, PhaseActions};
use fuel_station::{Nozzle, StationManager};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ShiftPhase {
    Scheduled,
    Open,
    PendingReview,
    Approved,
    Closed,
    Cancelled,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Reading {
    pub nozzle_id: String,
    pub initial_ml: i64,
    pub final_ml: Option<i64>,
    pub rollovers: i64,
    pub tiers: Vec<TierVolume>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TierVolume {
    pub tier_id: String,
    pub volume_ml: i64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Settlement {
    pub method: String,
    pub amount_irr: i64,
    pub reference: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Shift {
    pub name: String,
    pub starts_at: i64,
    pub ends_at: i64,
    pub employee_ids: Vec<String>,
    pub nozzle_ids: Vec<String>,
    pub status: ShiftPhase,
    pub equipment: Vec<Nozzle>,
    pub readings: Vec<Reading>,
    pub settlements: Vec<Settlement>,
    pub reconciliation_approved: bool,
    pub handover_note: String,
}
#[derive(Deserialize)]
struct Create {
    name: String,
    starts_at: i64,
    ends_at: i64,
    employee_ids: Vec<String>,
    nozzle_ids: Vec<String>,
}
#[derive(Deserialize)]
struct Transition {
    version: i64,
    to: ShiftPhase,
    readings: Option<Vec<Reading>>,
    settlements: Option<Vec<Settlement>>,
    handover_note: Option<String>,
}
#[derive(Serialize)]
pub struct ShiftChanged {
    pub phase: ShiftPhase,
    pub shift_id: String,
}
impl DomainEvent for ShiftChanged {
    const NAME: &'static str = "ShiftChanged";
}
#[derive(Serialize)]
pub struct ShiftSubmitted {
    pub shift_id: String,
}
impl DomainEvent for ShiftSubmitted {
    const NAME: &'static str = "ShiftSubmittedForReview";
}
struct Actions;
#[async_trait]
impl PhaseActions<ShiftPhase> for Actions {}
async fn transition_allowed(from: ShiftPhase, to: ShiftPhase) -> ApiResult<()> {
    use ShiftPhase::*;
    let graph = [
        (Scheduled, vec![Open, Cancelled]),
        (Open, vec![PendingReview]),
        (PendingReview, vec![Open, Approved]),
        (Approved, vec![Closed]),
        (Closed, vec![]),
        (Cancelled, vec![]),
    ];
    let controller = BasePhasedController::new(from, graph, Actions)
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
pub fn volume(reading: &Reading, nozzle: &Nozzle) -> ApiResult<i64> {
    let end = reading
        .final_ml
        .ok_or_else(|| ApiError::validation("Missing final meter reading"))?;
    require(
        reading.initial_ml >= 0
            && reading.initial_ml < nozzle.meter_modulus_ml
            && end >= 0
            && end < nozzle.meter_modulus_ml
            && (0..=1).contains(&reading.rollovers),
        "Invalid meter reading or rollover",
    )?;
    let volume = (end as i128) - (reading.initial_ml as i128)
        + (reading.rollovers as i128) * (nozzle.meter_modulus_ml as i128);
    require(
        volume >= 0 && volume <= i64::MAX as i128,
        "Final meter reading precedes initial reading; specify a verified rollover",
    )?;
    Ok(volume as i64)
}
pub struct ShiftManager;
impl ShiftManager {
    pub async fn submitted(tx: &mut Transaction, id: &str) -> ApiResult<Record<Shift>> {
        let record: Record<Shift> = tx.get("shifts", id).await?;
        require(
            record.data.status == ShiftPhase::PendingReview,
            "Shift must be pending review",
        )?;
        Ok(record)
    }
    pub async fn approve(tx: &mut Transaction, id: &str) -> ApiResult<()> {
        tx.actor.require("shift.approve")?;
        let mut shift = Self::submitted(tx, id).await?;
        transition_allowed(shift.data.status, ShiftPhase::Approved).await?;
        shift.data.status = ShiftPhase::Approved;
        shift.data.reconciliation_approved = true;
        let version = shift.version;
        tx.replace("shifts", &mut shift, version).await?;
        tx.emit(
            id,
            ShiftChanged {
                phase: ShiftPhase::Approved,
                shift_id: id.into(),
            },
        )
        .await
    }
}
pub struct ShiftModule;
#[async_trait]
impl Module for ShiftModule {
    fn name(&self) -> &'static str {
        "ShiftModule"
    }
    fn dependencies(&self) -> &'static [&'static str] {
        &["StationModule", "EmployeeModule"]
    }
    fn routes(&self) -> Router<AppState> {
        Router::new()
            .route("/shifts", get(list).post(create))
            .route("/shifts/{id}/transition", post(transition))
    }
    async fn initialize(&self, state: &AppState) -> ApiResult<()> {
        state.record_indexes("shifts").await?;
        state
            .index(
                "shifts",
                doc! {"org_id":1,"station_id":1,"data.starts_at":1,"data.ends_at":1},
                false,
            )
            .await
    }
}
async fn list(
    State(state): State<AppState>,
    Extension(actor): Extension<Actor>,
    Query(query): Query<PageQuery>,
) -> ApiResult<Json<Page<Record<Shift>>>> {
    actor.require("shift.view")?;
    Ok(Json(state.list(&actor, "shifts", query).await?))
}
async fn create(
    State(state): State<AppState>,
    Extension(actor): Extension<Actor>,
    Json(input): Json<Create>,
) -> ApiResult<Json<Record<Shift>>> {
    actor.require("shift.create")?;
    require(
        !input.name.trim().is_empty()
            && input.name.len() <= 120
            && input
                .ends_at
                .checked_sub(input.starts_at)
                .is_some_and(|duration| (60_000..=24 * 3_600_000).contains(&duration)),
        "Shift must have a name and last between 1 minute and 24 hours",
    )?;
    require(
        !input.employee_ids.is_empty() && !input.nozzle_ids.is_empty(),
        "Assign employees and nozzles",
    )?;
    require(
        input.nozzle_ids.iter().collect::<HashSet<_>>().len() == input.nozzle_ids.len()
            && input.employee_ids.iter().collect::<HashSet<_>>().len() == input.employee_ids.len(),
        "Duplicate assignment",
    )?;
    let mut tx = state.begin(&actor).await?;
    for id in &input.employee_ids {
        let user=tx.db().collection::<Document>("users").find_one(doc! {"org_id":&actor.org_id,"id":id,"data.active":true,"data.station_ids":&actor.station_id}).session(&mut tx.session).await?;
        require(user.is_some(), "Employee is unavailable at this station")?;
    }
    let conflict=tx.db().collection::<Document>("shifts").find_one(doc! {"org_id":&actor.org_id,"data.status":{"$ne":"Cancelled"},"data.starts_at":{"$lt":input.ends_at},"data.ends_at":{"$gt":input.starts_at},"$or":[{"data.employee_ids":{"$in":&input.employee_ids}},{"station_id":&actor.station_id,"data.nozzle_ids":{"$in":&input.nozzle_ids}}]}).session(&mut tx.session).await?;
    require(
        conflict.is_none(),
        "Employees or nozzles already have an overlapping shift",
    )?;
    let mut equipment = vec![];
    for id in &input.nozzle_ids {
        equipment.push(StationManager::nozzle(&mut tx, id).await?);
    }
    let record = Record::new(
        &actor,
        Shift {
            name: input.name,
            starts_at: input.starts_at,
            ends_at: input.ends_at,
            employee_ids: input.employee_ids,
            nozzle_ids: input.nozzle_ids,
            status: ShiftPhase::Scheduled,
            equipment,
            readings: vec![],
            settlements: vec![],
            reconciliation_approved: false,
            handover_note: String::new(),
        },
    );
    tx.insert("shifts", &record).await?;
    tx.emit(
        &record.id,
        ShiftChanged {
            phase: ShiftPhase::Scheduled,
            shift_id: record.id.clone(),
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
) -> ApiResult<Json<Record<Shift>>> {
    use ShiftPhase::*;
    let permission = match input.to {
        Open => "shift.open",
        PendingReview | Closed => "shift.close",
        Approved => "shift.approve",
        Scheduled | Cancelled => "shift.create",
    };
    actor.require(permission)?;
    let mut tx = state.begin(&actor).await?;
    let mut record: Record<Shift> = tx.get("shifts", &id).await?;
    let before = record.data.status;
    transition_allowed(before, input.to).await?;
    require(
        input.to != Approved,
        "Approve the reconciliation to approve this shift",
    )?;
    if input.to == Open {
        if before == PendingReview {
            actor.require("shift.approve")?;
            require(
                tx.find("reconciliations", doc! {"data.shift_id":&id})
                    .await?
                    .is_empty(),
                "This submission already has a reconciliation; use its correction workflow",
            )?;
        }
        let conflicts=tx.find("shifts",doc! {"id":{"$ne":&id},"data.status":{"$in":["Open","PendingReview","Approved"]},"data.nozzle_ids":{"$in":&record.data.nozzle_ids}}).await?;
        require(
            conflicts.is_empty(),
            "A nozzle is still owned by another active shift",
        )?;
        let readings = input
            .readings
            .ok_or_else(|| ApiError::validation("Initial readings are required"))?;
        require(
            readings.len() == record.data.nozzle_ids.len()
                && readings
                    .iter()
                    .map(|r| &r.nozzle_id)
                    .collect::<HashSet<_>>()
                    .len()
                    == readings.len(),
            "One reading per assigned nozzle is required",
        )?;
        for reading in &readings {
            let nozzle = record
                .data
                .equipment
                .iter()
                .find(|n| n.id == reading.nozzle_id)
                .ok_or_else(|| ApiError::validation("Unassigned nozzle"))?;
            require(
                reading.initial_ml >= 0
                    && reading.initial_ml < nozzle.meter_modulus_ml
                    && reading.final_ml.is_none()
                    && reading.rollovers == 0,
                "Invalid initial reading",
            )?;
        }
        for reading in &readings {
            let previous=tx.db().collection::<Document>("shifts").find_one(doc! {"org_id":&actor.org_id,"station_id":&actor.station_id,"id":{"$ne":&id},"data.status":"Closed","data.nozzle_ids":&reading.nozzle_id}).sort(doc! {"data.ends_at":-1}).session(&mut tx.session).await?;
            if let Some(raw) = previous {
                let previous: Record<Shift> = fuel_core::bson::from_document(raw)?;
                if let Some(last) = previous
                    .data
                    .readings
                    .iter()
                    .find(|r| r.nozzle_id == reading.nozzle_id)
                {
                    require(
                        last.final_ml == Some(reading.initial_ml),
                        "Initial reading must match the previous shift handover",
                    )?;
                }
            }
        }
        record.data.readings = readings;
    } else if input.to == PendingReview {
        let readings = input
            .readings
            .ok_or_else(|| ApiError::validation("Final readings are required"))?;
        require(
            readings.len() == record.data.readings.len()
                && readings
                    .iter()
                    .map(|r| &r.nozzle_id)
                    .collect::<HashSet<_>>()
                    .len()
                    == readings.len(),
            "One final reading per nozzle is required",
        )?;
        for reading in &readings {
            let original = record
                .data
                .readings
                .iter()
                .find(|r| r.nozzle_id == reading.nozzle_id)
                .ok_or_else(|| ApiError::validation("Unknown nozzle"))?;
            require(
                original.initial_ml == reading.initial_ml,
                "Initial readings cannot be silently changed",
            )?;
            let nozzle = record
                .data
                .equipment
                .iter()
                .find(|n| n.id == reading.nozzle_id)
                .ok_or_else(|| ApiError::validation("Unknown nozzle"))?;
            let dispensed = volume(reading, nozzle)?;
            let allocated = reading.tiers.iter().try_fold(0_i64, |sum, tier| {
                require(
                    tier.volume_ml >= 0 && nozzle.prices.iter().any(|p| p.id == tier.tier_id),
                    "Invalid price tier allocation",
                )?;
                sum.checked_add(tier.volume_ml)
                    .ok_or_else(|| ApiError::validation("Volume overflow"))
            })?;
            require(
                allocated == dispensed,
                "Price tier volumes must equal the meter volume",
            )?;
        }
        let settlements = input.settlements.unwrap_or_default();
        require(
            !settlements.is_empty()
                && settlements.iter().all(|s| {
                    s.amount_irr >= 0
                        && s.amount_irr <= 9_007_199_254_740_991
                        && ["cash", "card", "credit"].contains(&s.method.as_str())
                        && s.reference.len() <= 200
                }),
            "Valid settlement records are required",
        )?;
        record.data.readings = readings;
        record.data.settlements = settlements;
        tx.emit(
            &id,
            ShiftSubmitted {
                shift_id: id.clone(),
            },
        )
        .await?;
    }
    if input.to == Closed {
        require(
            record.data.reconciliation_approved,
            "Reconciliation approval is required",
        )?;
        let note = input.handover_note.unwrap_or_default();
        require(
            !note.trim().is_empty() && note.len() <= 2000,
            "A handover note is required",
        )?;
        record.data.handover_note = note;
    }
    record.data.status = input.to;
    tx.replace("shifts", &mut record, input.version).await?;
    tx.emit(
        &id,
        ShiftChanged {
            phase: input.to,
            shift_id: id.clone(),
        },
    )
    .await?;
    tx.commit().await?;
    Ok(Json(record))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rollover_is_explicit() {
        let nozzle = Nozzle {
            id: "n".into(),
            name: "n".into(),
            pump_id: "p".into(),
            tank_id: "t".into(),
            meter_modulus_ml: 1000,
            active: true,
            prices: vec![],
        };
        let mut reading = Reading {
            nozzle_id: "n".into(),
            initial_ml: 900,
            final_ml: Some(100),
            rollovers: 0,
            tiers: vec![],
        };
        assert!(volume(&reading, &nozzle).is_err());
        reading.rollovers = 1;
        assert_eq!(volume(&reading, &nozzle).unwrap(), 200);
    }
    #[tokio::test]
    async fn cannot_skip_approval() {
        assert!(transition_allowed(ShiftPhase::Open, ShiftPhase::Closed)
            .await
            .is_err());
    }
}
