use async_trait::async_trait;
use axum::{
    extract::{Query, State},
    http::{header, StatusCode},
    response::IntoResponse,
    routing::get,
    Extension, Json, Router,
};
use chrono::{Datelike, Duration, TimeZone, Utc};
use chrono_tz::Asia::Tehran;
use fuel_core::{
    bson::{self, doc, Document},
    *,
};
use futures_util::TryStreamExt;
use serde::{Deserialize, Serialize};

#[derive(Clone, Default, Deserialize)]
pub struct Filters {
    pub from: Option<i64>,
    pub to: Option<i64>,
    pub period: Option<String>,
    pub interval: Option<String>,
    pub station: Option<String>,
    pub tank: Option<String>,
    pub pump: Option<String>,
    pub nozzle: Option<String>,
    pub fuel_type: Option<String>,
    pub shift: Option<String>,
    pub status: Option<String>,
    pub compare: Option<bool>,
}
#[derive(Serialize)]
struct Dashboard {
    generated_at: i64,
    from: i64,
    to: i64,
    inventory_ml: i64,
    dispensed_ml: i64,
    active_shifts: u64,
    completed_shifts: u64,
    pending_approvals: u64,
    open_discrepancies: u64,
    financial_difference_irr: i64,
    correction_irr: i64,
    incoming_deliveries: u64,
    open_alerts: u64,
    employee_count: u64,
    series: Vec<Document>,
    inventory_history: Vec<Document>,
    fuel_distribution: Vec<Document>,
    shift_status: Vec<Document>,
    issue_categories: Vec<Document>,
    discrepancy_status: Vec<Document>,
    stations: Vec<Document>,
    recent: Vec<Document>,
    comparison: Option<Document>,
}
fn jalali(date: chrono::NaiveDate) -> (i32, u32, u32) {
    let mut gy = date.year();
    let gm = date.month() as i32;
    let gd = date.day() as i32;
    let mut jy = if gy > 1600 {
        gy -= 1600;
        979
    } else {
        gy -= 621;
        0
    };
    let gy2 = if gm > 2 { gy + 1 } else { gy };
    let months = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];
    let mut days = 365 * gy + (gy2 + 3) / 4 - (gy2 + 99) / 100 + (gy2 + 399) / 400 - 80
        + gd
        + months[(gm - 1) as usize];
    jy += 33 * (days / 12053);
    days %= 12053;
    jy += 4 * (days / 1461);
    days %= 1461;
    if days > 365 {
        jy += (days - 1) / 365;
        days = (days - 1) % 365;
    }
    let (jm, jd) = if days < 186 {
        (1 + days / 31, 1 + days % 31)
    } else {
        (7 + (days - 186) / 30, 1 + (days - 186) % 30)
    };
    (jy, jm as u32, jd as u32)
}
fn persian_start(date: chrono::NaiveDate, year: bool) -> chrono::NaiveDate {
    let mut day = date;
    for _ in 0..366 {
        let (_, month, date) = jalali(day);
        if date == 1 && (!year || month == 1) {
            break;
        }
        day -= Duration::days(1);
    }
    day
}
fn boundaries(query: &Filters) -> ApiResult<(i64, i64)> {
    if let (Some(from), Some(to)) = (query.from, query.to) {
        require(
            from >= 0
                && to
                    .checked_sub(from)
                    .is_some_and(|span| (1..=366 * 86_400_000).contains(&span)),
            "Date range must be within one year",
        )?;
        return Ok((from, to));
    }
    let local = Utc::now().with_timezone(&Tehran);
    let date = local.date_naive();
    let midnight = Tehran
        .from_local_datetime(&date.and_hms_opt(0, 0, 0).unwrap())
        .single()
        .ok_or_else(|| ApiError::validation("Invalid local date"))?;
    let (from, to) = match query.period.as_deref().unwrap_or("7d") {
        "today" => (midnight, midnight + Duration::days(1)),
        "yesterday" => (midnight - Duration::days(1), midnight),
        "week" => (
            midnight - Duration::days(((local.weekday().num_days_from_sunday() + 1) % 7) as i64),
            midnight + Duration::days(1),
        ),
        "30d" => (midnight - Duration::days(29), midnight + Duration::days(1)),
        "month" => (
            Tehran
                .from_local_datetime(&persian_start(date, false).and_hms_opt(0, 0, 0).unwrap())
                .single()
                .unwrap(),
            midnight + Duration::days(1),
        ),
        "previous_month" => {
            let first = persian_start(date, false);
            let previous = persian_start(first - Duration::days(1), false);
            (
                Tehran
                    .from_local_datetime(&previous.and_hms_opt(0, 0, 0).unwrap())
                    .single()
                    .unwrap(),
                Tehran
                    .from_local_datetime(&first.and_hms_opt(0, 0, 0).unwrap())
                    .single()
                    .unwrap(),
            )
        }
        "year" => (
            Tehran
                .from_local_datetime(&persian_start(date, true).and_hms_opt(0, 0, 0).unwrap())
                .single()
                .unwrap(),
            midnight + Duration::days(1),
        ),
        "7d" => (midnight - Duration::days(6), midnight + Duration::days(1)),
        _ => return Err(ApiError::validation("Unknown reporting period")),
    };
    Ok((from.timestamp_millis(), to.timestamp_millis()))
}
async fn aggregate(
    state: &AppState,
    col: &str,
    pipeline: Vec<Document>,
) -> ApiResult<Vec<Document>> {
    Ok(state
        .db
        .collection::<Document>(col)
        .aggregate(pipeline)
        .await?
        .try_collect()
        .await?)
}
async fn count(state: &AppState, col: &str, filter: Document) -> ApiResult<u64> {
    Ok(state
        .db
        .collection::<Document>(col)
        .count_documents(filter)
        .await?)
}
async fn sum(state: &AppState, col: &str, filter: Document, field: &str) -> ApiResult<i64> {
    let rows = aggregate(
        state,
        col,
        vec![
            doc! {"$match":filter},
            doc! {"$group":{"_id":bson::Bson::Null,"value":{"$sum":field}}},
        ],
    )
    .await?;
    Ok(rows
        .first()
        .and_then(|d| {
            d.get_i64("value")
                .ok()
                .or_else(|| d.get_i32("value").ok().map(i64::from))
        })
        .unwrap_or(0))
}
async fn group(
    state: &AppState,
    col: &str,
    filter: Document,
    field: &str,
) -> ApiResult<Vec<Document>> {
    aggregate(
        state,
        col,
        vec![
            doc! {"$match":filter},
            doc! {"$group":{"_id":field,"value":{"$sum":1}}},
            doc! {"$project":{"_id":0,"name":"$_id","value":1}},
        ],
    )
    .await
}
async fn report(state: &AppState, actor: &Actor, query: Filters) -> ApiResult<Dashboard> {
    actor.require("dashboard.view")?;
    let (from, to) = boundaries(&query)?;
    let stations = if query.station.as_deref() == Some("all") {
        actor.station_ids.clone()
    } else {
        vec![query
            .station
            .clone()
            .unwrap_or_else(|| actor.station_id.clone())]
    };
    for station in &stations {
        fuel_permission::resolve(state, &actor.org_id, &actor.user_id, Some(station))
            .await?
            .require("dashboard.view")?;
    }
    let scope = doc! {"org_id":&actor.org_id,"station_id":{"$in":&stations}};
    let mut historical = scope.clone();
    historical.insert("created_at", doc! {"$gte":from,"$lt":to});
    let mut movement = historical.clone();
    let mut balances = scope.clone();
    let mut deliveries = historical.clone();
    let mut shifts = historical.clone();
    let mut reconciliations = historical.clone();
    let mut tanks = Vec::<String>::new();
    if let Some(tank) = &query.tank {
        tanks.push(tank.clone());
    }
    if query.fuel_type.is_some() || query.pump.is_some() || query.nozzle.is_some() {
        let configs: Vec<Document> = state
            .db
            .collection::<Document>("stations")
            .find(doc! {"org_id":&actor.org_id,"id":{"$in":&stations}})
            .await?
            .try_collect()
            .await?;
        let mut matching = vec![];
        for config in configs {
            let data = config
                .get_document("data")
                .map_err(|_| ApiError::validation("Invalid station configuration"))?;
            if let Ok(list) = data.get_array("tanks") {
                for tank in list.iter().filter_map(|v| v.as_document()) {
                    if query
                        .fuel_type
                        .as_ref()
                        .is_none_or(|fuel| tank.get_str("fuel_type_id").ok() == Some(fuel.as_str()))
                    {
                        matching.push(tank.get_str("id").unwrap_or_default().to_string());
                    }
                }
            }
            if query.pump.is_some() || query.nozzle.is_some() {
                let mut nozzle_tanks = vec![];
                if let Ok(list) = data.get_array("nozzles") {
                    for nozzle in list.iter().filter_map(|v| v.as_document()) {
                        if query.pump.as_ref().is_none_or(|pump| {
                            nozzle.get_str("pump_id").ok() == Some(pump.as_str())
                        }) && query
                            .nozzle
                            .as_ref()
                            .is_none_or(|id| nozzle.get_str("id").ok() == Some(id.as_str()))
                        {
                            nozzle_tanks
                                .push(nozzle.get_str("tank_id").unwrap_or_default().to_string());
                        }
                    }
                }
                matching.retain(|t| nozzle_tanks.contains(t));
            }
        }
        if tanks.is_empty() {
            tanks = matching;
        } else {
            tanks.retain(|t| matching.contains(t));
        }
        if tanks.is_empty() {
            tanks.push("__no_match__".into());
        }
    }
    if !tanks.is_empty() {
        for filter in [&mut movement, &mut balances, &mut deliveries] {
            filter.insert("data.tank_id", doc! {"$in":&tanks});
        }
        shifts.insert("data.equipment.tank_id", doc! {"$in":&tanks});
    }
    if let Some(pump) = &query.pump {
        shifts.insert("data.equipment.pump_id", pump);
        movement.insert("data.pump_id", pump);
    }
    if let Some(nozzle) = &query.nozzle {
        shifts.insert("data.nozzle_ids", nozzle);
        movement.insert("data.nozzle_id", nozzle);
    }
    if let Some(shift) = &query.shift {
        shifts.insert("id", shift);
        reconciliations.insert("data.shift_id", shift);
        movement.insert("data.shift_id", shift);
    }
    if let Some(status) = &query.status {
        shifts.insert("data.status", status);
    }
    if query.status.is_some()
        || query.shift.is_some()
        || query.nozzle.is_some()
        || query.pump.is_some()
        || !tanks.is_empty()
    {
        let mut selection = shifts.clone();
        selection.remove("created_at");
        let ids = state
            .db
            .collection::<Document>("shifts")
            .distinct("id", selection)
            .await?;
        reconciliations.insert("data.shift_id", doc! {"$in":ids.clone()});
        if query.status.is_some() {
            movement.insert("data.shift_id", doc! {"$in":ids});
        }
    }
    let movement_scope = movement.clone();
    let mut dispensed = movement.clone();
    dispensed.insert("data.kind", "dispensing");
    let inventory_ml = sum(
        state,
        "inventory_balances",
        balances.clone(),
        "$data.volume_ml",
    )
    .await?;
    let dispensed_ml = -sum(state, "inventory_movements", dispensed, "$data.quantity_ml").await?;
    let mut active = shifts.clone();
    active.remove("created_at");
    active.insert("data.status", "Open");
    let mut completed = shifts.clone();
    completed.insert("data.status", "Closed");
    let mut pending = shifts.clone();
    pending.remove("created_at");
    pending.insert("data.status", "PendingReview");
    let mut discrepancies = reconciliations.clone();
    discrepancies.insert("data.status", doc! {"$ne":"Completed"});
    discrepancies.insert(
        "$expr",
        doc! {"$gt":[{"$abs":"$data.difference_irr"},"$data.tolerance_irr"]},
    );
    let mut alerts = scope.clone();
    alerts.insert("data.status", doc! {"$ne":"Resolved"});
    let unit = query.interval.as_deref().unwrap_or("day");
    require(
        ["day", "week", "month", "year"].contains(&unit),
        "Invalid aggregation interval",
    )?;
    let series=aggregate(state,"inventory_movements",vec![doc! {"$match":movement},doc! {"$group":{"_id":{"$dateTrunc":{"date":{"$toDate":"$data.measured_at"},"unit":unit,"timezone":"Asia/Tehran"}},"dispensed_ml":{"$sum":{"$cond":[{"$eq":["$data.kind","dispensing"]},{"$multiply":["$data.quantity_ml",-1]},0]}},"delivery_ml":{"$sum":{"$cond":[{"$eq":["$data.kind","delivery"]},"$data.quantity_ml",0]}},"net_ml":{"$sum":"$data.quantity_ml"}}},doc! {"$sort":{"_id":1}},doc! {"$project":{"_id":0,"at":{"$toLong":"$_id"},"dispensed_ml":1,"delivery_ml":1,"net_ml":1}}]).await?;
    let fuel_distribution=aggregate(state,"inventory_balances",vec![doc! {"$match":balances.clone()},doc! {"$lookup":{"from":"stations","let":{"station":"$station_id","org":"$org_id","tank":"$data.tank_id"},"pipeline":[{"$match":{"$expr":{"$and":[{"$eq":["$id","$$station"]},{"$eq":["$org_id","$$org"]}]}}},{"$unwind":"$data.tanks"},{"$match":{"$expr":{"$eq":["$data.tanks.id","$$tank"]}}},{"$unwind":"$data.fuel_types"},{"$match":{"$expr":{"$eq":["$data.tanks.fuel_type_id","$data.fuel_types.id"]}}}],"as":"configuration"}},doc! {"$unwind":"$configuration"},doc! {"$group":{"_id":"$configuration.data.fuel_types.name","value":{"$sum":"$data.volume_ml"}}},doc! {"$project":{"_id":0,"name":"$_id","value":1}}]).await?;
    let mut history_scope = balances;
    history_scope.remove("created_at");
    history_scope.insert("data.measured_at", doc! {"$lt":to});
    let history_start = bson::DateTime::from_millis(from);
    let history_end = bson::DateTime::from_millis(to);
    let inventory_history=aggregate(state,"inventory_movements",vec![
        doc! {"$match":history_scope},doc! {"$sort":{"data.measured_at":1,"created_at":1}},
        doc! {"$project":{"station_id":1,"tank_id":"$data.tank_id","volume_ml":"$data.resulting_volume_ml","at":{"$max":[history_start,{"$dateTrunc":{"date":{"$toDate":"$data.measured_at"},"unit":"day","timezone":"Asia/Tehran"}}]}}},
        doc! {"$group":{"_id":{"station":"$station_id","tank":"$tank_id","at":"$at"},"volume_ml":{"$last":"$volume_ml"}}},
        doc! {"$project":{"_id":0,"station_id":"$_id.station","tank_id":"$_id.tank","at":"$_id.at","volume_ml":1}},
        doc! {"$densify":{"field":"at","partitionByFields":["station_id","tank_id"],"range":{"step":1,"unit":"day","bounds":[history_start,history_end]}}},
        doc! {"$fill":{"partitionByFields":["station_id","tank_id"],"sortBy":{"at":1},"output":{"volume_ml":{"method":"locf"}}}},
        doc! {"$group":{"_id":"$at","volume_ml":{"$sum":"$volume_ml"}}},doc! {"$sort":{"_id":1}},doc! {"$project":{"_id":0,"at":{"$toLong":"$_id"},"volume_ml":1}}
    ]).await?;
    let station_comparison=aggregate(state,"inventory_movements",vec![doc! {"$match":movement_scope.clone()},doc! {"$group":{"_id":"$station_id","dispensed_ml":{"$sum":{"$cond":[{"$eq":["$data.kind","dispensing"]},{"$multiply":["$data.quantity_ml",-1]},0]}},"delivery_ml":{"$sum":{"$cond":[{"$eq":["$data.kind","delivery"]},"$data.quantity_ml",0]}}}},doc! {"$project":{"_id":0,"name":"$_id","dispensed_ml":1,"delivery_ml":1}}]).await?;
    let recent: Vec<Document> = state
        .db
        .collection::<Document>("audit_logs")
        .find(scope.clone())
        .projection(doc! {"_id":0})
        .sort(doc! {"created_at":-1})
        .limit(6)
        .await?
        .try_collect()
        .await?;
    let comparison = if query.compare.unwrap_or(false) {
        let mut previous = movement_scope;
        previous.insert("created_at", doc! {"$gte":from-(to-from),"$lt":from});
        previous.insert("data.kind", "dispensing");
        Some(
            doc! {"from":from-(to-from),"to":from,"dispensed_ml":-sum(state,"inventory_movements",previous,"$data.quantity_ml").await?},
        )
    } else {
        None
    };
    Ok(Dashboard {
        generated_at: now(),
        from,
        to,
        inventory_ml,
        dispensed_ml,
        active_shifts: count(state, "shifts", active).await?,
        completed_shifts: count(state, "shifts", completed).await?,
        pending_approvals: count(state, "shifts", pending).await?,
        open_discrepancies: count(state, "reconciliations", discrepancies).await?,
        financial_difference_irr: sum(
            state,
            "reconciliations",
            reconciliations.clone(),
            "$data.difference_irr",
        )
        .await?,
        correction_irr: sum(state, "corrections", historical.clone(), "$data.delta_irr").await?,
        incoming_deliveries: count(state, "deliveries", deliveries).await?,
        open_alerts: count(state, "alerts", alerts).await?,
        employee_count: count(
            state,
            "users",
            doc! {"org_id":&actor.org_id,"data.active":true,"data.station_ids":{"$in":&stations}},
        )
        .await?,
        series,
        inventory_history,
        fuel_distribution,
        shift_status: group(state, "shifts", shifts, "$data.status").await?,
        issue_categories: group(state, "alerts", scope, "$data.kind").await?,
        discrepancy_status: group(state, "reconciliations", reconciliations, "$data.status")
            .await?,
        stations: station_comparison,
        recent,
        comparison,
    })
}
pub struct DashboardModule;
#[async_trait]
impl Module for DashboardModule {
    fn name(&self) -> &'static str {
        "DashboardModule"
    }
    fn dependencies(&self) -> &'static [&'static str] {
        &["PermissionModule"]
    }
    fn routes(&self) -> Router<AppState> {
        Router::new()
            .route("/dashboard", get(dashboard))
            .route("/dashboard/export", get(export))
    }
}
async fn dashboard(
    State(state): State<AppState>,
    Extension(actor): Extension<Actor>,
    Query(query): Query<Filters>,
) -> ApiResult<Json<Dashboard>> {
    Ok(Json(report(&state, &actor, query).await?))
}
async fn export(
    State(state): State<AppState>,
    Extension(actor): Extension<Actor>,
    Query(query): Query<Filters>,
) -> ApiResult<impl IntoResponse> {
    actor.require("report.export")?;
    let report = report(&state, &actor, query).await?;
    let mut csv = String::from("\u{feff}utc_timestamp,dispensed_ml,delivery_ml,net_ml\r\n");
    for row in report.series {
        csv.push_str(&format!(
            "{},{},{},{}\r\n",
            row.get_i64("at").unwrap_or(0),
            row.get_i64("dispensed_ml").unwrap_or(0),
            row.get_i64("delivery_ml").unwrap_or(0),
            row.get_i64("net_ml").unwrap_or(0)
        ));
    }
    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "text/csv; charset=utf-8"),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=fuel-ops-report.csv",
            ),
        ],
        csv,
    ))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn persian_boundaries_follow_nowruz_and_month_start() {
        let date = chrono::NaiveDate::from_ymd_opt(2025, 3, 21).unwrap();
        assert_eq!(jalali(date), (1404, 1, 1));
        assert_eq!(jalali(date - Duration::days(1)), (1403, 12, 30));
        let later = chrono::NaiveDate::from_ymd_opt(2025, 4, 25).unwrap();
        assert_eq!(persian_start(later, true), date);
        assert_eq!(
            persian_start(later, false),
            chrono::NaiveDate::from_ymd_opt(2025, 4, 21).unwrap()
        );
    }
    #[test]
    fn rejects_reversed_dates() {
        assert!(boundaries(&Filters {
            from: Some(20),
            to: Some(10),
            ..Default::default()
        })
        .is_err());
    }
    #[test]
    fn rejects_unbounded_range() {
        assert!(boundaries(&Filters {
            from: Some(0),
            to: Some(367 * 86_400_000),
            ..Default::default()
        })
        .is_err());
    }
}
