use async_trait::async_trait;
use axum::{
    extract::{Query, State},
    routing::get,
    Extension, Json, Router,
};
use fuel_core::{bson::doc, events::DomainEvent, *};
use fuel_station::StationManager;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Balance {
    pub tank_id: String,
    pub volume_ml: i64,
    pub source: String,
    pub measured_at: i64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Movement {
    pub tank_id: String,
    pub quantity_ml: i64,
    pub kind: String,
    pub reference: String,
    pub source: String,
    pub measured_at: i64,
    pub resulting_volume_ml: i64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VolumeMeasurement {
    pub tank_id: String,
    pub volume_ml: i64,
    pub source: String,
    pub measured_at: i64,
}
#[async_trait]
pub trait InventoryDataProvider: Send + Sync {
    async fn measure(&self, tank_id: &str) -> ApiResult<VolumeMeasurement>;
}
pub struct EnteredMeasurement(pub VolumeMeasurement);
#[async_trait]
impl InventoryDataProvider for EnteredMeasurement {
    async fn measure(&self, tank_id: &str) -> ApiResult<VolumeMeasurement> {
        require(
            self.0.tank_id == tank_id
                && ["manual", "simulated"].contains(&self.0.source.as_str())
                && self.0.volume_ml >= 0,
            "Invalid manual or simulated volume measurement",
        )?;
        Ok(self.0.clone())
    }
}
#[derive(Deserialize)]
struct Measurement {
    tank_id: String,
    volume_ml: i64,
    source: String,
    measured_at: i64,
    note: String,
    version: Option<i64>,
}
#[derive(Serialize)]
pub struct InventoryUpdated {
    pub tank_id: String,
    pub volume_ml: i64,
    pub source: String,
}
impl DomainEvent for InventoryUpdated {
    const NAME: &'static str = "InventoryUpdated";
}
pub struct InventoryManager;
impl InventoryManager {
    pub async fn dispense(
        tx: &mut Transaction,
        tank_id: &str,
        volume_ml: i64,
        reconciliation_id: &str,
        shift_id: &str,
        nozzle_id: &str,
        pump_id: &str,
    ) -> ApiResult<()> {
        let reference = format!("{reconciliation_id}:{nozzle_id}");
        Self::change(
            tx,
            tank_id,
            -volume_ml,
            &reference,
            "dispensing",
            "calculated",
        )
        .await?;
        let mut filter = tx.actor.scope();
        filter.insert("data.reference", &reference);
        filter.insert("data.kind", "dispensing");
        tx.db().collection::<fuel_core::bson::Document>("inventory_movements").update_one(filter,doc! {"$set":{"data.shift_id":shift_id,"data.nozzle_id":nozzle_id,"data.pump_id":pump_id}}).session(&mut tx.session).await?;
        Ok(())
    }
    pub async fn change(
        tx: &mut Transaction,
        tank_id: &str,
        delta: i64,
        reference: &str,
        kind: &str,
        source: &str,
    ) -> ApiResult<()> {
        let tank = StationManager::tank(tx, tank_id).await?;
        let mut balance: Record<Balance> = tx.get("inventory_balances", tank_id).await?;
        let next = balance
            .data
            .volume_ml
            .checked_add(delta)
            .ok_or_else(|| ApiError::validation("Inventory overflow"))?;
        require(next>=0 && next<=tank.capacity_ml,"Recorded inventory would be below zero or above tank capacity; investigate measurements first")?;
        let version = balance.version;
        balance.data.volume_ml = next;
        balance.data.source = "calculated".into();
        balance.data.measured_at = now();
        tx.replace("inventory_balances", &mut balance, version)
            .await?;
        let movement = Record::new(
            &tx.actor,
            Movement {
                tank_id: tank_id.into(),
                quantity_ml: delta,
                kind: kind.into(),
                reference: reference.into(),
                source: source.into(),
                measured_at: now(),
                resulting_volume_ml: next,
            },
        );
        tx.insert("inventory_movements", &movement).await?;
        tx.emit(
            tank_id,
            InventoryUpdated {
                tank_id: tank_id.into(),
                volume_ml: next,
                source: "calculated".into(),
            },
        )
        .await
    }
}
pub struct InventoryModule;
#[async_trait]
impl Module for InventoryModule {
    fn name(&self) -> &'static str {
        "InventoryModule"
    }
    fn dependencies(&self) -> &'static [&'static str] {
        &["StationModule"]
    }
    fn routes(&self) -> Router<AppState> {
        Router::new()
            .route("/inventory", get(list).post(measure))
            .route("/inventory/movements", get(movements))
    }
    async fn initialize(&self, state: &AppState) -> ApiResult<()> {
        for col in [
            "inventory_balances",
            "inventory_movements",
            "inventory_measurements",
        ] {
            state.record_indexes(col).await?;
        }
        state
            .index(
                "inventory_movements",
                doc! {"org_id":1,"station_id":1,"data.tank_id":1,"data.reference":1,"data.kind":1},
                true,
            )
            .await
    }
}
async fn list(
    State(state): State<AppState>,
    Extension(actor): Extension<Actor>,
    Query(query): Query<PageQuery>,
) -> ApiResult<Json<Page<Record<Balance>>>> {
    actor.require("inventory.view")?;
    Ok(Json(state.list(&actor, "inventory_balances", query).await?))
}
async fn movements(
    State(state): State<AppState>,
    Extension(actor): Extension<Actor>,
    Query(query): Query<PageQuery>,
) -> ApiResult<Json<Page<Record<Movement>>>> {
    actor.require("inventory.view")?;
    Ok(Json(
        state.list(&actor, "inventory_movements", query).await?,
    ))
}
async fn measure(
    State(state): State<AppState>,
    Extension(actor): Extension<Actor>,
    Json(input): Json<Measurement>,
) -> ApiResult<Json<Record<Balance>>> {
    actor.require("inventory.manage")?;
    EnteredMeasurement(VolumeMeasurement {
        tank_id: input.tank_id.clone(),
        volume_ml: input.volume_ml,
        source: input.source.clone(),
        measured_at: input.measured_at,
    })
    .measure(&input.tank_id)
    .await?;
    require(
        ["manual", "simulated"].contains(&input.source.as_str())
            && input.measured_at <= now()
            && input.measured_at >= now() - 86_400_000
            && !input.note.trim().is_empty(),
        "Use a recent manual or simulated volume measurement with a note",
    )?;
    let mut tx = state.begin(&actor).await?;
    let tank = StationManager::tank(&mut tx, &input.tank_id).await?;
    require(
        input.volume_ml >= 0 && input.volume_ml <= tank.capacity_ml,
        "Volume exceeds tank capacity",
    )?;
    let existing = tx
        .find("inventory_balances", doc! {"id":&input.tank_id})
        .await?;
    let mut previous = 0;
    let mut record = if let Some(raw) = existing.into_iter().next() {
        let row: Record<Balance> = fuel_core::bson::from_document(raw)?;
        require(
            input.measured_at >= row.data.measured_at,
            "Measurement predates the current recorded balance; use a documented correction",
        )?;
        previous = row.data.volume_ml;
        row
    } else {
        let mut row = Record::new(
            &actor,
            Balance {
                tank_id: input.tank_id.clone(),
                volume_ml: 0,
                source: input.source.clone(),
                measured_at: input.measured_at,
            },
        );
        row.id = input.tank_id.clone();
        row
    };
    record.data = Balance {
        tank_id: input.tank_id.clone(),
        volume_ml: input.volume_ml,
        source: input.source.clone(),
        measured_at: input.measured_at,
    };
    if let Some(version) = input.version {
        tx.replace("inventory_balances", &mut record, version)
            .await?;
    } else {
        tx.insert("inventory_balances", &record).await?;
    }
    let reference = fuel_core::id();
    let movement = Record::new(
        &actor,
        Movement {
            tank_id: input.tank_id.clone(),
            quantity_ml: input.volume_ml - previous,
            kind: "measurement".into(),
            reference: reference.clone(),
            source: input.source.clone(),
            measured_at: input.measured_at,
            resulting_volume_ml: input.volume_ml,
        },
    );
    tx.insert("inventory_movements", &movement).await?;
    tx.insert("inventory_measurements",&Record::new(&actor,doc! {"tank_id":&input.tank_id,"volume_ml":input.volume_ml,"source":&input.source,"measured_at":input.measured_at,"note":input.note,"reference":reference})).await?;
    tx.emit(
        &input.tank_id,
        InventoryUpdated {
            tank_id: input.tank_id.clone(),
            volume_ml: input.volume_ml,
            source: input.source,
        },
    )
    .await?;
    tx.commit().await?;
    Ok(Json(record))
}
