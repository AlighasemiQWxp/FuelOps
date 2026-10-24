use async_trait::async_trait;
use axum::{
    extract::{Path, Query, State},
    routing::{get, post},
    Extension, Json, Router,
};
use fuel_core::{bson::doc, events::DomainEvent, *};
use fuel_inventory::InventoryManager;
use fuel_phases::{BasePhasedController, PhaseActions};
use fuel_shift::{volume, Shift, ShiftManager};
use fuel_station::StationManager;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ReconciliationPhase {
    CollectingReadings,
    Calculating,
    ReviewingDiscrepancies,
    Approval,
    Completed,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Reconciliation {
    pub shift_id: String,
    pub shift_name: String,
    pub status: ReconciliationPhase,
    pub volume_ml: i64,
    pub expected_irr: i64,
    pub settled_irr: i64,
    pub difference_irr: i64,
    pub tolerance_irr: i64,
    pub investigation: String,
    pub shift_version: i64,
    pub submitted_by: String,
    pub approved_by: Option<String>,
}
#[derive(Serialize)]
pub struct ReconciliationChanged {
    pub phase: ReconciliationPhase,
    pub difference_irr: i64,
}
impl DomainEvent for ReconciliationChanged {
    const NAME: &'static str = "ReconciliationChanged";
}
#[derive(Serialize)]
pub struct DiscrepancyDetected {
    pub difference_irr: i64,
    pub description: String,
}
impl DomainEvent for DiscrepancyDetected {
    const NAME: &'static str = "DiscrepancyDetected";
}
#[derive(Deserialize)]
struct Create {
    shift_id: String,
}
#[derive(Deserialize)]
struct Transition {
    version: i64,
    to: ReconciliationPhase,
    investigation: Option<String>,
}
#[derive(Deserialize)]
struct Correction {
    version: i64,
    #[serde(default)]
    delta_irr: i64,
    #[serde(default)]
    inventory_delta_ml: i64,
    tank_id: Option<String>,
    reason: String,
}
struct Actions;
#[async_trait]
impl PhaseActions<ReconciliationPhase> for Actions {}
async fn permits(from: ReconciliationPhase, to: ReconciliationPhase) -> ApiResult<()> {
    use ReconciliationPhase::*;
    let controller = BasePhasedController::new(
        from,
        [
            (CollectingReadings, vec![Calculating]),
            (Calculating, vec![ReviewingDiscrepancies]),
            (ReviewingDiscrepancies, vec![Approval]),
            (Approval, vec![ReviewingDiscrepancies, Completed]),
            (Completed, vec![]),
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
pub fn calculate(shift: &Shift) -> ApiResult<(i64, i64, i64)> {
    let mut volume_total = 0_i128;
    let mut expected = 0_i128;
    let mut settled = 0_i128;
    for reading in &shift.readings {
        let nozzle = shift
            .equipment
            .iter()
            .find(|n| n.id == reading.nozzle_id)
            .ok_or_else(|| ApiError::validation("Unknown nozzle"))?;
        volume_total += volume(reading, nozzle)? as i128;
        let mut numerator = 0_i128;
        for tier in &reading.tiers {
            let price = nozzle
                .prices
                .iter()
                .find(|p| p.id == tier.tier_id)
                .ok_or_else(|| ApiError::validation("Unknown historical price tier"))?;
            numerator += (tier.volume_ml as i128) * (price.irr_per_liter as i128);
        }
        expected += (numerator + 500) / 1000;
    }
    for settlement in &shift.settlements {
        settled += settlement.amount_irr as i128;
    }
    require(
        [volume_total, expected, settled]
            .iter()
            .all(|v| *v >= 0 && *v <= 9_007_199_254_740_991_i128),
        "Calculation exceeds supported range",
    )?;
    Ok((volume_total as i64, expected as i64, settled as i64))
}
pub struct ReconciliationModule;
#[async_trait]
impl Module for ReconciliationModule {
    fn name(&self) -> &'static str {
        "ReconciliationModule"
    }
    fn dependencies(&self) -> &'static [&'static str] {
        &["ShiftModule", "InventoryModule"]
    }
    fn routes(&self) -> Router<AppState> {
        Router::new()
            .route("/reconciliations", get(list).post(create))
            .route("/reconciliations/{id}/transition", post(transition))
            .route(
                "/reconciliations/{id}/corrections",
                get(corrections).post(correct),
            )
    }
    async fn initialize(&self, state: &AppState) -> ApiResult<()> {
        state.record_indexes("reconciliations").await?;
        state.record_indexes("corrections").await?;
        state
            .index(
                "reconciliations",
                doc! {"org_id":1,"station_id":1,"data.shift_id":1},
                true,
            )
            .await
    }
}
async fn list(
    State(state): State<AppState>,
    Extension(actor): Extension<Actor>,
    Query(query): Query<PageQuery>,
) -> ApiResult<Json<Page<Record<Reconciliation>>>> {
    actor.require("reconciliation.view")?;
    Ok(Json(state.list(&actor, "reconciliations", query).await?))
}
async fn create(
    State(state): State<AppState>,
    Extension(actor): Extension<Actor>,
    Json(input): Json<Create>,
) -> ApiResult<Json<Record<Reconciliation>>> {
    actor.require("reconciliation.manage")?;
    let mut tx = state.begin(&actor).await?;
    let shift = ShiftManager::submitted(&mut tx, &input.shift_id).await?;
    let config = StationManager::configuration(&mut tx).await?;
    let record = Record::new(
        &actor,
        Reconciliation {
            shift_id: shift.id,
            shift_name: shift.data.name,
            status: ReconciliationPhase::CollectingReadings,
            volume_ml: 0,
            expected_irr: 0,
            settled_irr: 0,
            difference_irr: 0,
            tolerance_irr: config.data.discrepancy_tolerance_irr,
            investigation: String::new(),
            shift_version: shift.version,
            submitted_by: actor.user_id.clone(),
            approved_by: None,
        },
    );
    tx.insert("reconciliations", &record).await?;
    tx.commit().await?;
    Ok(Json(record))
}
async fn transition(
    State(state): State<AppState>,
    Extension(actor): Extension<Actor>,
    Path(id): Path<String>,
    Json(input): Json<Transition>,
) -> ApiResult<Json<Record<Reconciliation>>> {
    use ReconciliationPhase::*;
    actor.require("reconciliation.manage")?;
    if input.to == Completed {
        actor.require("reconciliation.approve")?;
        actor.require("shift.approve")?;
    }
    let mut tx = state.begin(&actor).await?;
    let mut record: Record<Reconciliation> = tx.get("reconciliations", &id).await?;
    permits(record.data.status, input.to).await?;
    let shift = ShiftManager::submitted(&mut tx, &record.data.shift_id).await?;
    require(
        shift.version == record.data.shift_version,
        "Shift changed; recreate the reconciliation",
    )?;
    if input.to == Calculating {
        let (volume_ml, expected, settled) = calculate(&shift.data)?;
        record.data.volume_ml = volume_ml;
        record.data.expected_irr = expected;
        record.data.settled_irr = settled;
        record.data.difference_irr = settled - expected;
        if record.data.difference_irr.unsigned_abs() > record.data.tolerance_irr as u64 {
            tx.emit(
                &id,
                DiscrepancyDetected {
                    difference_irr: record.data.difference_irr,
                    description: format!(
                        "اختلاف تسویه شیفت {} نیازمند بررسی است",
                        record.data.shift_name
                    ),
                },
            )
            .await?;
        }
    }
    if let Some(note) = input.investigation {
        require(note.len() <= 4000, "Investigation note is too long")?;
        record.data.investigation = note;
    }
    if matches!(input.to, Approval | Completed)
        && record.data.difference_irr.unsigned_abs() > record.data.tolerance_irr as u64
    {
        require(
            !record.data.investigation.trim().is_empty(),
            "Investigate and explain the discrepancy before approval",
        )?;
    }
    if input.to == Completed {
        for reading in &shift.data.readings {
            let nozzle = shift
                .data
                .equipment
                .iter()
                .find(|n| n.id == reading.nozzle_id)
                .ok_or_else(|| ApiError::validation("Unknown nozzle"))?;
            InventoryManager::dispense(
                &mut tx,
                &nozzle.tank_id,
                volume(reading, nozzle)?,
                &id,
                &shift.id,
                &nozzle.id,
                &nozzle.pump_id,
            )
            .await?;
        }
        ShiftManager::approve(&mut tx, &shift.id).await?;
        record.data.approved_by = Some(actor.user_id.clone());
    }
    record.data.status = input.to;
    tx.replace("reconciliations", &mut record, input.version)
        .await?;
    tx.emit(
        &id,
        ReconciliationChanged {
            phase: input.to,
            difference_irr: record.data.difference_irr,
        },
    )
    .await?;
    tx.commit().await?;
    Ok(Json(record))
}
async fn corrections(
    State(state): State<AppState>,
    Extension(actor): Extension<Actor>,
    Path(id): Path<String>,
) -> ApiResult<Json<Vec<fuel_core::bson::Document>>> {
    actor.require("reconciliation.view")?;
    state
        .get::<Reconciliation>(&actor, "reconciliations", &id)
        .await?;
    use futures_util::TryStreamExt;
    let mut filter = actor.scope();
    filter.insert("data.reconciliation_id", id);
    Ok(Json(
        state
            .db
            .collection("corrections")
            .find(filter)
            .sort(doc! {"created_at":-1})
            .limit(100)
            .await?
            .try_collect()
            .await?,
    ))
}
async fn correct(
    State(state): State<AppState>,
    Extension(actor): Extension<Actor>,
    Path(id): Path<String>,
    Json(input): Json<Correction>,
) -> ApiResult<Json<serde_json::Value>> {
    actor.require("reconciliation.approve")?;
    require(
        !input.reason.trim().is_empty()
            && input.reason.len() <= 4000
            && (input.delta_irr != 0 || input.inventory_delta_ml != 0)
            && input.delta_irr.unsigned_abs() <= 9_007_199_254_740_991_u64
            && input.inventory_delta_ml.unsigned_abs() <= 9_007_199_254_740_991_u64,
        "A nonzero adjustment and reason are required",
    )?;
    let mut tx = state.begin(&actor).await?;
    let mut record: Record<Reconciliation> = tx.get("reconciliations", &id).await?;
    require(
        record.data.status == ReconciliationPhase::Completed,
        "Only finalized records use correction entries",
    )?;
    tx.replace("reconciliations", &mut record, input.version)
        .await?;
    let correction = Record::new(
        &actor,
        doc! {"reconciliation_id":&id,"delta_irr":input.delta_irr,"inventory_delta_ml":input.inventory_delta_ml,"tank_id":&input.tank_id,"reason":input.reason,"approved_by":&actor.user_id},
    );
    if input.inventory_delta_ml != 0 {
        actor.require("inventory.manage")?;
        let tank_id = input
            .tank_id
            .as_deref()
            .ok_or_else(|| ApiError::validation("Select the tank being corrected"))?;
        let shift: Record<Shift> = tx.get("shifts", &record.data.shift_id).await?;
        require(
            shift
                .data
                .equipment
                .iter()
                .any(|nozzle| nozzle.tank_id == tank_id),
            "The tank does not belong to this reconciliation",
        )?;
        InventoryManager::change(
            &mut tx,
            tank_id,
            input.inventory_delta_ml,
            &correction.id,
            "correction",
            "manual",
        )
        .await?;
    }
    tx.insert("corrections", &correction).await?;
    tx.commit().await?;
    Ok(Json(
        serde_json::json!({"id":correction.id,"version":record.version}),
    ))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn money_uses_integer_rounding() {
        use fuel_shift::{Reading, Settlement, ShiftPhase, TierVolume};
        use fuel_station::{Nozzle, PriceTier};
        let shift = Shift {
            name: "test".into(),
            starts_at: 0,
            ends_at: 1,
            employee_ids: vec![],
            nozzle_ids: vec!["n".into()],
            status: ShiftPhase::PendingReview,
            equipment: vec![Nozzle {
                id: "n".into(),
                name: "n".into(),
                pump_id: "p".into(),
                tank_id: "t".into(),
                meter_modulus_ml: 10000,
                active: true,
                prices: vec![
                    PriceTier {
                        id: "a".into(),
                        name: "a".into(),
                        irr_per_liter: 15000,
                    },
                    PriceTier {
                        id: "b".into(),
                        name: "b".into(),
                        irr_per_liter: 30000,
                    },
                ],
            }],
            readings: vec![Reading {
                nozzle_id: "n".into(),
                initial_ml: 0,
                final_ml: Some(1500),
                rollovers: 0,
                tiers: vec![
                    TierVolume {
                        tier_id: "a".into(),
                        volume_ml: 1000,
                    },
                    TierVolume {
                        tier_id: "b".into(),
                        volume_ml: 500,
                    },
                ],
            }],
            settlements: vec![Settlement {
                method: "card".into(),
                amount_irr: 30000,
                reference: String::new(),
            }],
            reconciliation_approved: false,
            handover_note: String::new(),
        };
        assert_eq!(calculate(&shift).unwrap(), (1500, 30000, 30000));
    }
}
