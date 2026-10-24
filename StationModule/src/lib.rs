use async_trait::async_trait;
use axum::{
    extract::{Path, State},
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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FuelType {
    pub id: String,
    pub name: String,
    pub color: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PriceTier {
    pub id: String,
    pub name: String,
    pub irr_per_liter: i64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Tank {
    pub id: String,
    pub name: String,
    pub fuel_type_id: String,
    pub capacity_ml: i64,
    pub low_threshold_ml: i64,
    pub calibration_reference: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Pump {
    pub id: String,
    pub name: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Nozzle {
    pub id: String,
    pub name: String,
    pub pump_id: String,
    pub tank_id: String,
    pub meter_modulus_ml: i64,
    pub active: bool,
    pub prices: Vec<PriceTier>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Station {
    pub name: String,
    pub city: String,
    pub timezone: String,
    pub demo: bool,
    pub fuel_types: Vec<FuelType>,
    pub tanks: Vec<Tank>,
    pub pumps: Vec<Pump>,
    pub nozzles: Vec<Nozzle>,
    pub discrepancy_tolerance_irr: i64,
    pub delivery_tolerance_ml: i64,
}
#[derive(Serialize)]
struct StationConfigured {
    station_id: String,
}
impl DomainEvent for StationConfigured {
    const NAME: &'static str = "StationConfigured";
}
pub struct StationManager;
impl StationManager {
    pub async fn configuration(tx: &mut Transaction) -> ApiResult<Record<Station>> {
        let id = tx.actor.station_id.clone();
        tx.get("stations", &id).await
    }
    pub async fn nozzle(tx: &mut Transaction, id: &str) -> ApiResult<Nozzle> {
        Self::configuration(tx)
            .await?
            .data
            .nozzles
            .into_iter()
            .find(|n| n.id == id && n.active)
            .ok_or_else(|| ApiError::validation("Nozzle is missing or inactive"))
    }
    pub async fn tank(tx: &mut Transaction, id: &str) -> ApiResult<Tank> {
        Self::configuration(tx)
            .await?
            .data
            .tanks
            .into_iter()
            .find(|t| t.id == id)
            .ok_or_else(|| ApiError::validation("Tank not found"))
    }
}
fn validate(station: &Station) -> ApiResult<()> {
    require(
        !station.name.trim().is_empty()
            && station.name.len() < 160
            && station.timezone == "Asia/Tehran",
        "Provide a station name and Asia/Tehran timezone",
    )?;
    require(
        station.discrepancy_tolerance_irr >= 0 && station.delivery_tolerance_ml >= 0,
        "Thresholds cannot be negative",
    )?;
    let fuel: HashSet<_> = station.fuel_types.iter().map(|f| &f.id).collect();
    let tanks: HashSet<_> = station.tanks.iter().map(|t| &t.id).collect();
    let pumps: HashSet<_> = station.pumps.iter().map(|p| &p.id).collect();
    let nozzles: HashSet<_> = station.nozzles.iter().map(|n| &n.id).collect();
    require(
        fuel.len() == station.fuel_types.len()
            && tanks.len() == station.tanks.len()
            && pumps.len() == station.pumps.len()
            && nozzles.len() == station.nozzles.len(),
        "Equipment identifiers must be unique",
    )?;
    for tank in &station.tanks {
        require(
            fuel.contains(&tank.fuel_type_id)
                && tank.capacity_ml > 0
                && tank.capacity_ml <= 9_007_199_254_740_991
                && tank.low_threshold_ml >= 0
                && tank.low_threshold_ml < tank.capacity_ml,
            "Invalid tank configuration",
        )?;
    }
    for nozzle in &station.nozzles {
        require(
            tanks.contains(&nozzle.tank_id)
                && pumps.contains(&nozzle.pump_id)
                && nozzle.meter_modulus_ml > 0
                && nozzle.meter_modulus_ml <= 9_007_199_254_740_991
                && !nozzle.prices.is_empty(),
            "Invalid nozzle configuration",
        )?;
        let ids: HashSet<_> = nozzle.prices.iter().map(|p| &p.id).collect();
        require(
            ids.len() == nozzle.prices.len()
                && nozzle
                    .prices
                    .iter()
                    .all(|p| p.irr_per_liter >= 0 && p.irr_per_liter < 1_000_000_000),
            "Invalid price tiers",
        )?;
    }
    Ok(())
}
pub struct StationModule;
#[async_trait]
impl Module for StationModule {
    fn name(&self) -> &'static str {
        "StationModule"
    }
    fn routes(&self) -> Router<AppState> {
        Router::new()
            .route("/stations", get(list).post(create))
            .route("/stations/{id}", axum::routing::put(update))
    }
    async fn initialize(&self, state: &AppState) -> ApiResult<()> {
        state.record_indexes("stations").await
    }
}
async fn list(
    State(state): State<AppState>,
    Extension(actor): Extension<Actor>,
) -> ApiResult<Json<serde_json::Value>> {
    actor.require("station.view")?;
    let raw: Vec<Document> = state
        .db
        .collection::<Document>("stations")
        .find(doc! {"org_id":&actor.org_id,"id":{"$in":&actor.station_ids}})
        .limit(100)
        .await?
        .try_collect()
        .await?;
    let items: Vec<Record<Station>> = raw
        .into_iter()
        .map(bson::from_document)
        .collect::<Result<_, _>>()?;
    Ok(Json(serde_json::json!({"total":items.len(),"items":items})))
}
async fn create(
    State(state): State<AppState>,
    Extension(actor): Extension<Actor>,
    Json(input): Json<Station>,
) -> ApiResult<Json<Record<Station>>> {
    actor.require("station.manage")?;
    actor.require("permission.manage")?;
    validate(&input)?;
    let mut new_actor = actor.clone();
    new_actor.station_id = fuel_core::id();
    new_actor.station_ids.push(new_actor.station_id.clone());
    let mut record = Record::new(&new_actor, input);
    record.id = new_actor.station_id.clone();
    let mut tx = state.begin(&new_actor).await?;
    tx.insert("stations", &record).await?;
    let role_id = fuel_core::id();
    tx.db().collection::<Document>("roles").insert_one(doc! {"id":&role_id,"org_id":&actor.org_id,"station_id":&record.id,"version":1_i64,"created_at":now(),"updated_at":now(),"data":{"name":"مدیر جایگاه","permissions":actor.permissions.iter().cloned().collect::<Vec<_>>()}}).session(&mut tx.session).await?;
    tx.db().collection::<Document>("users").update_one(doc! {"org_id":&actor.org_id,"id":&actor.user_id},doc! {"$addToSet":{"data.station_ids":&record.id,"data.role_ids":role_id},"$inc":{"version":1_i64}}).session(&mut tx.session).await?;
    tx.emit(
        &record.id,
        StationConfigured {
            station_id: record.id.clone(),
        },
    )
    .await?;
    tx.commit().await?;
    Ok(Json(record))
}
#[derive(Deserialize)]
struct Update {
    version: i64,
    data: Station,
}
async fn update(
    State(state): State<AppState>,
    Extension(actor): Extension<Actor>,
    Path(id): Path<String>,
    Json(input): Json<Update>,
) -> ApiResult<Json<Record<Station>>> {
    actor.require("station.manage")?;
    validate(&input.data)?;
    require(id == actor.station_id, "Select this station before editing")?;
    let mut tx = state.begin(&actor).await?;
    let mut record: Record<Station> = tx.get("stations", &id).await?;
    let active = tx
        .find(
            "shifts",
            doc! {"data.status":{"$in":["Open","PendingReview"]}},
        )
        .await?;
    require(
        active.is_empty(),
        "Finish active shifts before changing equipment or prices",
    )?;
    require(record.data.tanks.iter().all(|old|input.data.tanks.iter().any(|new|new.id==old.id && new.fuel_type_id==old.fuel_type_id)) && record.data.nozzles.iter().all(|old|input.data.nozzles.iter().any(|new|new.id==old.id)),"Retain existing equipment identifiers; deactivate unused nozzles instead of deleting history")?;
    for tank in &input.data.tanks {
        let balances = tx
            .find("inventory_balances", doc! {"data.tank_id":&tank.id})
            .await?;
        for balance in balances {
            if let Ok(data) = balance.get_document("data") {
                require(
                    data.get_i64("volume_ml").unwrap_or(0) <= tank.capacity_ml,
                    "Tank capacity is below recorded inventory",
                )?;
            }
        }
    }
    record.data = input.data;
    tx.replace("stations", &mut record, input.version).await?;
    tx.emit(
        &id,
        StationConfigured {
            station_id: id.clone(),
        },
    )
    .await?;
    tx.commit().await?;
    Ok(Json(record))
}
