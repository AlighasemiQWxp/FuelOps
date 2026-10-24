use argon2::{
    password_hash::{PasswordHasher, SaltString},
    Argon2,
};
use fuel_alert::{Alert, History};
use fuel_core::{
    bson::{self, doc, Document},
    database::Config,
    *,
};
use fuel_delivery::{Delivery, DeliveryPhase};
use fuel_inventory::{Balance, Movement};
use fuel_permission::{Role, User, PERMISSIONS};
use fuel_reconciliation::{Reconciliation, ReconciliationPhase};
use fuel_shift::{Reading, Settlement, Shift, ShiftPhase, TierVolume};
use fuel_station::{FuelType, Nozzle, PriceTier, Pump, Station, Tank};
use std::collections::HashSet;

fn actor(station: &str) -> Actor {
    Actor {
        user_id: "demo-admin".into(),
        org_id: "fuelops-demo".into(),
        station_id: station.into(),
        station_ids: vec!["demo-tehran".into(), "demo-karaj".into()],
        permissions: PERMISSIONS.iter().map(|p| p.to_string()).collect(),
        name: "مدیر نمونه".into(),
    }
}
fn record<T>(actor: &Actor, id: &str, data: T, at: i64) -> Record<T> {
    let mut row = Record::new(actor, data);
    row.id = id.into();
    row.created_at = at;
    row.updated_at = at;
    row
}
async fn insert<T: serde::Serialize>(
    tx: &mut Transaction,
    col: &str,
    row: &Record<T>,
) -> ApiResult<()> {
    tx.db()
        .collection::<Document>(col)
        .insert_one(bson::to_document(row)?)
        .session(&mut tx.session)
        .await?;
    Ok(())
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();
    let password = std::env::var("DEMO_PASSWORD").unwrap_or_default();
    require(
        password.len() >= 12 && !password.starts_with("replace-"),
        "Set DEMO_PASSWORD to a unique password of at least 12 characters",
    )?;
    let state = AppState::connect(Config::from_env()?).await?;
    if state
        .db
        .collection::<Document>("organizations")
        .find_one(doc! {"_id":"fuelops-demo","seed_complete":true})
        .await?
        .is_some()
    {
        return Ok(());
    }
    require(
        state
            .db
            .collection::<Document>("users")
            .count_documents(doc! {})
            .await?
            == 0,
        "Demo seed requires an empty database and never overwrites existing users",
    )?;
    CoreModule::new()
        .register(fuel_permission::PermissionModule)
        .register(fuel_auth::AuthModule)
        .register(fuel_station::StationModule)
        .register(fuel_employee::EmployeeModule)
        .register(fuel_inventory::InventoryModule)
        .register(fuel_shift::ShiftModule)
        .register(fuel_reconciliation::ReconciliationModule)
        .register(fuel_delivery::DeliveryModule)
        .register(fuel_alert::AlertModule)
        .register(fuel_audit::AuditModule)
        .initialize(&state)
        .await?;
    let hash = Argon2::default()
        .hash_password(
            password.as_bytes(),
            &SaltString::generate(&mut rand::rngs::OsRng),
        )
        .map_err(|_| ApiError::validation("Cannot hash password"))?
        .to_string();
    let baseline = now() - 40 * 86_400_000;
    let mut tx = state.begin(&actor("demo-tehran")).await?;
    tx.db().collection::<Document>("organizations").insert_one(doc! {"_id":"fuelops-demo","name":"مجموعه نمونه سوخت آریا","fictional":true,"seed_complete":false}).session(&mut tx.session).await?;
    let admin = record(
        &actor("demo-tehran"),
        "demo-admin",
        User {
            name: "آرمان رضایی".into(),
            email: "owner@fuelops.demo".into(),
            password_hash: hash.clone(),
            active: true,
            station_ids: vec!["demo-tehran".into(), "demo-karaj".into()],
            role_ids: vec!["demo-tehran-owner".into(), "demo-karaj-owner".into()],
        },
        baseline,
    );
    insert(&mut tx, "users", &admin).await?;
    for (index, station_id, station_name, city) in [
        (0, "demo-tehran", "جایگاه آریا · تهران", "تهران"),
        (1, "demo-karaj", "جایگاه آریا · البرز", "کرج"),
    ] {
        let owner = actor(station_id);
        let all: HashSet<String> = PERMISSIONS.iter().map(|p| p.to_string()).collect();
        for (suffix, name, permissions) in [
            ("owner", "مالک جایگاه", all.clone()),
            (
                "supervisor",
                "سرپرست",
                all.iter()
                    .filter(|p| !p.starts_with("permission.") && !p.starts_with("employee."))
                    .cloned()
                    .collect(),
            ),
            (
                "operator",
                "اپراتور",
                [
                    "station.view",
                    "shift.view",
                    "shift.open",
                    "shift.close",
                    "delivery.view",
                    "delivery.create",
                    "inventory.view",
                    "alert.view",
                    "alert.manage",
                    "dashboard.view",
                ]
                .iter()
                .map(|p| p.to_string())
                .collect(),
            ),
            (
                "viewer",
                "مشاهده‌گر",
                [
                    "station.view",
                    "shift.view",
                    "delivery.view",
                    "inventory.view",
                    "reconciliation.view",
                    "dashboard.view",
                    "alert.view",
                ]
                .iter()
                .map(|p| p.to_string())
                .collect(),
            ),
        ] {
            insert(
                &mut tx,
                "roles",
                &record(
                    &owner,
                    &format!("{station_id}-{suffix}"),
                    Role {
                        name: name.into(),
                        permissions,
                    },
                    baseline,
                ),
            )
            .await?;
        }
        for (suffix, name) in [
            ("supervisor", "نیلوفر احمدی"),
            ("operator", "مهدی کریمی"),
            ("viewer", "سارا محمدی"),
        ] {
            insert(
                &mut tx,
                "users",
                &record(
                    &owner,
                    &format!("{station_id}-{suffix}-user"),
                    User {
                        name: name.into(),
                        email: format!("{suffix}{index}@fuelops.demo"),
                        password_hash: hash.clone(),
                        active: true,
                        station_ids: vec![station_id.into()],
                        role_ids: vec![format!("{station_id}-{suffix}")],
                    },
                    baseline,
                ),
            )
            .await?;
        }
        let t1 = format!("{station_id}-t1");
        let t2 = format!("{station_id}-t2");
        let pump = format!("{station_id}-p1");
        let nozzles: Vec<Nozzle> = (1..=3)
            .map(|n| Nozzle {
                id: format!("{station_id}-n{n}"),
                name: format!("نازل {n}"),
                pump_id: pump.clone(),
                tank_id: if n == 2 { t2.clone() } else { t1.clone() },
                meter_modulus_ml: 1_000_000_000,
                active: true,
                prices: vec![
                    PriceTier {
                        id: "quota".into(),
                        name: "سهمیه‌ای".into(),
                        irr_per_liter: 15000,
                    },
                    PriceTier {
                        id: "free".into(),
                        name: "آزاد".into(),
                        irr_per_liter: 30000,
                    },
                ],
            })
            .collect();
        let station = Station {
            name: station_name.into(),
            city: city.into(),
            timezone: "Asia/Tehran".into(),
            demo: true,
            fuel_types: vec![
                FuelType {
                    id: "petrol".into(),
                    name: "بنزین معمولی".into(),
                    color: "#0d9488".into(),
                },
                FuelType {
                    id: "premium".into(),
                    name: "بنزین سوپر".into(),
                    color: "#f59e0b".into(),
                },
            ],
            tanks: vec![
                Tank {
                    id: t1.clone(),
                    name: "مخزن بنزین معمولی".into(),
                    fuel_type_id: "petrol".into(),
                    capacity_ml: 60_000_000,
                    low_threshold_ml: 8_000_000,
                    calibration_reference: None,
                },
                Tank {
                    id: t2.clone(),
                    name: "مخزن بنزین سوپر".into(),
                    fuel_type_id: "premium".into(),
                    capacity_ml: 50_000_000,
                    low_threshold_ml: 6_000_000,
                    calibration_reference: None,
                },
            ],
            pumps: vec![Pump {
                id: pump,
                name: "سکوی یک".into(),
            }],
            nozzles: nozzles.clone(),
            discrepancy_tolerance_irr: 10000,
            delivery_tolerance_ml: 100000,
        };
        insert(
            &mut tx,
            "stations",
            &record(&owner, station_id, station, baseline),
        )
        .await?;
        let mut stock = [32_000_000_i64, 27_000_000_i64];
        let mut meter = [100_000_000_i64, 200_000_000_i64];
        for (tank, balance) in [(&t1, stock[0]), (&t2, stock[1])] {
            insert(
                &mut tx,
                "inventory_movements",
                &record(
                    &owner,
                    &fuel_core::id(),
                    Movement {
                        tank_id: tank.clone(),
                        quantity_ml: balance,
                        kind: "measurement".into(),
                        reference: "opening-demo".into(),
                        source: "simulated".into(),
                        measured_at: baseline,
                        resulting_volume_ml: balance,
                    },
                    baseline,
                ),
            )
            .await?;
        }
        for day in 0..30_i64 {
            let at = now() - (30 - day) * 86_400_000;
            let shift_id = format!("{station_id}-history-{day}");
            let reconciliation_id = format!("{shift_id}-reconciliation");
            let volumes = [
                (650 + day * 17 % 400 + index * 120) * 1000,
                (450 + day * 23 % 300 + index * 80) * 1000,
            ];
            let mut readings = vec![];
            let mut expected = 0;
            for n in 0..2 {
                let quota = volumes[n] * 3 / 5;
                let free = volumes[n] - quota;
                expected += (quota * 15000 + free * 30000) / 1000;
                readings.push(Reading {
                    nozzle_id: nozzles[n].id.clone(),
                    initial_ml: meter[n],
                    final_ml: Some(meter[n] + volumes[n]),
                    rollovers: 0,
                    tiers: vec![
                        TierVolume {
                            tier_id: "quota".into(),
                            volume_ml: quota,
                        },
                        TierVolume {
                            tier_id: "free".into(),
                            volume_ml: free,
                        },
                    ],
                });
                meter[n] += volumes[n];
                stock[n] -= volumes[n];
                insert(
                    &mut tx,
                    "inventory_movements",
                    &record(
                        &owner,
                        &fuel_core::id(),
                        Movement {
                            tank_id: nozzles[n].tank_id.clone(),
                            quantity_ml: -volumes[n],
                            kind: "dispensing".into(),
                            reference: reconciliation_id.clone(),
                            source: "simulated".into(),
                            measured_at: at,
                            resulting_volume_ml: stock[n],
                        },
                        at,
                    ),
                )
                .await?;
                if day % 7 == 5 {
                    let delivery_id = format!("{station_id}-delivery-{day}-{n}");
                    let before = stock[n];
                    stock[n] += 8_000_000;
                    insert(
                        &mut tx,
                        "deliveries",
                        &record(
                            &owner,
                            &delivery_id,
                            Delivery {
                                document_number: format!("DEMO-{index}-{day}-{n}"),
                                supplier: "تأمین‌کننده فرضی آریا".into(),
                                tank_id: nozzles[n].tank_id.clone(),
                                fuel_type_id: if n == 0 {
                                    "petrol".into()
                                } else {
                                    "premium".into()
                                },
                                declared_ml: 8_000_000,
                                before_ml: Some(before),
                                after_ml: Some(stock[n]),
                                measured_ml: Some(8_000_000),
                                difference_ml: Some(0),
                                tolerance_ml: 100000,
                                source: "simulated".into(),
                                status: DeliveryPhase::Completed,
                                note: "اطلاعات نمایشی و غیرواقعی".into(),
                                approved_by: Some("demo-admin".into()),
                            },
                            at + 1000,
                        ),
                    )
                    .await?;
                    insert(
                        &mut tx,
                        "inventory_movements",
                        &record(
                            &owner,
                            &fuel_core::id(),
                            Movement {
                                tank_id: nozzles[n].tank_id.clone(),
                                quantity_ml: 8_000_000,
                                kind: "delivery".into(),
                                reference: delivery_id,
                                source: "simulated".into(),
                                measured_at: at + 1000,
                                resulting_volume_ml: stock[n],
                            },
                            at + 1000,
                        ),
                    )
                    .await?;
                }
            }
            for nozzle in &nozzles[..2] {
                tx.db().collection::<Document>("inventory_movements").update_one(doc! {"org_id":&owner.org_id,"station_id":station_id,"data.reference":&reconciliation_id,"data.tank_id":&nozzle.tank_id,"data.kind":"dispensing"},doc! {"$set":{"data.shift_id":&shift_id,"data.nozzle_id":&nozzle.id,"data.pump_id":&nozzle.pump_id}}).session(&mut tx.session).await?;
            }
            let difference = if day % 9 == 3 { -45000 } else { 0 };
            let shift = Shift {
                name: format!("شیفت روز · {}", day + 1),
                starts_at: at - 8 * 3_600_000,
                ends_at: at,
                employee_ids: vec![format!("{station_id}-operator-user")],
                nozzle_ids: nozzles[..2].iter().map(|n| n.id.clone()).collect(),
                status: ShiftPhase::Closed,
                equipment: nozzles[..2].to_vec(),
                readings,
                settlements: vec![Settlement {
                    method: "card".into(),
                    amount_irr: expected + difference,
                    reference: format!("DEMO-POS-{day}"),
                }],
                reconciliation_approved: true,
                handover_note: "تحویل نمونه با ثبت کنتورها".into(),
            };
            insert(&mut tx, "shifts", &record(&owner, &shift_id, shift, at)).await?;
            insert(
                &mut tx,
                "reconciliations",
                &record(
                    &owner,
                    &reconciliation_id,
                    Reconciliation {
                        shift_id,
                        shift_name: format!("شیفت روز · {}", day + 1),
                        status: ReconciliationPhase::Completed,
                        volume_ml: volumes.iter().sum(),
                        expected_irr: expected,
                        settled_irr: expected + difference,
                        difference_irr: difference,
                        tolerance_irr: 10000,
                        investigation: if difference != 0 {
                            "اختلاف نمونه بررسی شد؛ کارمزد تسویه ثبت نشده بود".into()
                        } else {
                            String::new()
                        },
                        shift_version: 1,
                        submitted_by: format!("{station_id}-supervisor-user"),
                        approved_by: Some("demo-admin".into()),
                    },
                    at,
                ),
            )
            .await?;
        }
        for n in 0..2 {
            insert(
                &mut tx,
                "inventory_balances",
                &record(
                    &owner,
                    &nozzles[n].tank_id,
                    Balance {
                        tank_id: nozzles[n].tank_id.clone(),
                        volume_ml: stock[n],
                        source: "simulated".into(),
                        measured_at: now() - 3_600_000,
                    },
                    now() - 3_600_000,
                ),
            )
            .await?;
        }
        for (suffix, status, start, end) in [
            (
                "active",
                ShiftPhase::Open,
                now() - 2 * 3_600_000,
                now() + 6 * 3_600_000,
            ),
            (
                "next",
                ShiftPhase::Scheduled,
                now() + 22 * 3_600_000,
                now() + 30 * 3_600_000,
            ),
        ] {
            insert(
                &mut tx,
                "shifts",
                &record(
                    &owner,
                    &format!("{station_id}-{suffix}"),
                    Shift {
                        name: if suffix == "active" {
                            "شیفت جاری".into()
                        } else {
                            "شیفت فردا".into()
                        },
                        starts_at: start,
                        ends_at: end,
                        employee_ids: vec![format!("{station_id}-operator-user")],
                        nozzle_ids: vec![nozzles[2].id.clone()],
                        status,
                        equipment: vec![nozzles[2].clone()],
                        readings: if suffix == "active" {
                            vec![Reading {
                                nozzle_id: nozzles[2].id.clone(),
                                initial_ml: 500_000_000,
                                final_ml: None,
                                rollovers: 0,
                                tiers: vec![],
                            }]
                        } else {
                            vec![]
                        },
                        settlements: vec![],
                        reconciliation_approved: false,
                        handover_note: String::new(),
                    },
                    now() - 2 * 3_600_000,
                ),
            )
            .await?;
        }
        for (number, kind, title, status) in [
            (1, "nozzle", "نیاز به بررسی نازل شماره ۲", "Open"),
            (2, "delivery", "بررسی سند تحویل قبلی", "Acknowledged"),
            (3, "reading", "بازبینی ثبت کنتور", "Resolved"),
        ] {
            insert(
                &mut tx,
                "alerts",
                &record(
                    &owner,
                    &format!("{station_id}-issue-{number}"),
                    Alert {
                        kind: kind.into(),
                        severity: if number == 1 {
                            "high".into()
                        } else {
                            "medium".into()
                        },
                        title: title.into(),
                        description: "رویداد فرضی برای نمایش گردش کار؛ پایش ایمنی صنعتی نیست"
                            .into(),
                        entity_id: nozzles[1].id.clone(),
                        assigned_to: Some(format!("{station_id}-supervisor-user")),
                        status: status.into(),
                        resolution: if number == 3 {
                            "بررسی و اصلاح نمونه انجام شد".into()
                        } else {
                            String::new()
                        },
                        source: "simulated".into(),
                        history: vec![History {
                            at: now() - number * 3_600_000,
                            actor: "سامانه نمونه".into(),
                            status: status.into(),
                            note: "اطلاعات نمایشی".into(),
                        }],
                    },
                    now() - number * 3_600_000,
                ),
            )
            .await?;
        }
        tx.db().collection::<Document>("audit_logs").insert_one(doc! {"id":fuel_core::id(),"org_id":&owner.org_id,"station_id":station_id,"actor_id":"demo-admin","actor_name":"آرمان رضایی","entity_id":station_id,"collection":"stations","action":"demo_seeded","before_version":0_i64,"after_version":1_i64,"created_at":now()}).session(&mut tx.session).await?;
    }
    tx.db()
        .collection::<Document>("organizations")
        .update_one(
            doc! {"_id":"fuelops-demo"},
            doc! {"$set":{"seed_complete":true}},
        )
        .session(&mut tx.session)
        .await?;
    tx.commit().await?;
    Ok(())
}
