use argon2::{
    password_hash::{PasswordHasher, SaltString},
    Argon2,
};
use fuel_core::{
    bson::{doc, Document},
    database::Config,
    *,
};
use fuel_permission::{Role, User, PERMISSIONS};
use fuel_station::{FuelType, Nozzle, PriceTier, Pump, Station, Tank};
use reqwest::{Client, StatusCode};
use serde_json::{json, Value};

struct Fixture {
    state: AppState,
    client: Client,
    base: String,
    origin: String,
    org: String,
    station: String,
    other_station: String,
    user: String,
    password: String,
}
impl Fixture {
    async fn new() -> Self {
        let database = std::env::var("MONGODB_DATABASE")
            .expect("Set MONGODB_DATABASE to the running server's disposable _test database");
        assert!(
            database.ends_with("_test"),
            "Integration tests require a disposable database ending in _test"
        );
        let state = AppState::connect(Config::from_env().unwrap())
            .await
            .unwrap();
        let org = format!("test-{}", id());
        let station = id();
        let other_station = id();
        let user = id();
        let password = format!("test-password-{}", id());
        let actor = Actor {
            user_id: user.clone(),
            org_id: org.clone(),
            station_id: station.clone(),
            station_ids: vec![station.clone()],
            permissions: PERMISSIONS.iter().map(|p| p.to_string()).collect(),
            name: "Test owner".into(),
        };
        let mut tx = state.begin(&actor).await.unwrap();
        let role = Record::new(
            &actor,
            Role {
                name: "Owner".into(),
                permissions: actor.permissions.clone(),
            },
        );
        tx.insert("roles", &role).await.unwrap();
        let hash = Argon2::default()
            .hash_password(
                password.as_bytes(),
                &SaltString::generate(&mut rand::rngs::OsRng),
            )
            .unwrap()
            .to_string();
        let mut record = Record::new(
            &actor,
            User {
                name: "Owner".into(),
                email: "owner@test.invalid".into(),
                password_hash: hash.clone(),
                active: true,
                station_ids: vec![station.clone()],
                role_ids: vec![role.id],
            },
        );
        record.id = user.clone();
        tx.insert("users", &record).await.unwrap();
        let viewer_role = Record::new(
            &actor,
            Role {
                name: "Viewer".into(),
                permissions: ["station.view", "shift.view", "dashboard.view"]
                    .iter()
                    .map(|p| p.to_string())
                    .collect(),
            },
        );
        tx.insert("roles", &viewer_role).await.unwrap();
        tx.insert(
            "users",
            &Record::new(
                &actor,
                User {
                    name: "Viewer".into(),
                    email: "viewer@test.invalid".into(),
                    password_hash: hash,
                    active: true,
                    station_ids: vec![station.clone()],
                    role_ids: vec![viewer_role.id],
                },
            ),
        )
        .await
        .unwrap();
        let mut station_record = Record::new(
            &actor,
            Station {
                name: "Test station".into(),
                city: "Tehran".into(),
                timezone: "Asia/Tehran".into(),
                demo: true,
                fuel_types: vec![FuelType {
                    id: "petrol".into(),
                    name: "Petrol".into(),
                    color: "#000000".into(),
                }],
                tanks: vec![Tank {
                    id: "tank".into(),
                    name: "Tank".into(),
                    fuel_type_id: "petrol".into(),
                    capacity_ml: 1000000,
                    low_threshold_ml: 10000,
                    calibration_reference: None,
                }],
                pumps: vec![Pump {
                    id: "pump".into(),
                    name: "Pump".into(),
                }],
                nozzles: vec![Nozzle {
                    id: "nozzle".into(),
                    name: "Nozzle".into(),
                    pump_id: "pump".into(),
                    tank_id: "tank".into(),
                    meter_modulus_ml: 1000000,
                    active: true,
                    prices: vec![PriceTier {
                        id: "price".into(),
                        name: "Standard".into(),
                        irr_per_liter: 30000,
                    }],
                }],
                discrepancy_tolerance_irr: 100,
                delivery_tolerance_ml: 100,
            },
        );
        station_record.id = station.clone();
        tx.insert("stations", &station_record).await.unwrap();
        let mut balance = Record::new(
            &actor,
            fuel_inventory::Balance {
                tank_id: "tank".into(),
                volume_ml: 500000,
                source: "simulated".into(),
                measured_at: now(),
            },
        );
        balance.id = "tank".into();
        tx.insert("inventory_balances", &balance).await.unwrap();
        tx.commit().await.unwrap();
        Self {
            state,
            client: Client::builder().cookie_store(true).build().unwrap(),
            base: std::env::var("TEST_API_BASE")
                .unwrap_or_else(|_| "http://127.0.0.1:8080/api/v1".into()),
            origin: std::env::var("WEB_ORIGIN").unwrap_or_else(|_| "http://localhost:5173".into()),
            org,
            station,
            other_station,
            user,
            password,
        }
    }
    async fn login(&self, email: &str) -> String {
        let response = self
            .client
            .post(format!("{}/auth/login", self.base))
            .header("Origin", &self.origin)
            .json(&json!({"organization":self.org,"email":email,"password":self.password}))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        response.json::<Value>().await.unwrap()["access_token"]
            .as_str()
            .unwrap()
            .into()
    }
    async fn post(&self, token: &str, path: &str, body: Value) -> reqwest::Response {
        self.client
            .post(format!("{}{path}", self.base))
            .bearer_auth(token)
            .header("X-Station-Id", &self.station)
            .json(&body)
            .send()
            .await
            .unwrap()
    }
    async fn get(&self, token: &str, path: &str) -> Value {
        let response = self
            .client
            .get(format!("{}{path}", self.base))
            .bearer_auth(token)
            .header("X-Station-Id", &self.station)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        response.json().await.unwrap()
    }
}
#[tokio::test]
#[ignore = "Requires the API and a disposable MongoDB replica-set database ending in _test"]
async fn authorization_transactions_workflows_and_reports() {
    let f = Fixture::new().await;
    let viewer = f.login("viewer@test.invalid").await;
    assert_eq!(
        f.post(&viewer, "/shifts", json!({})).await.status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let denied=f.post(&viewer,"/shifts",json!({"name":"Unauthorized","starts_at":now(),"ends_at":now()+3600000,"employee_ids":[f.user],"nozzle_ids":["nozzle"]})).await;
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);
    let owner = f.login("owner@test.invalid").await;
    let isolated = f
        .client
        .get(format!("{}/shifts", f.base))
        .bearer_auth(&owner)
        .header("X-Station-Id", &f.other_station)
        .send()
        .await
        .unwrap();
    assert_eq!(isolated.status(), StatusCode::FORBIDDEN);
    let start = now();
    let body = json!({"name":"Test shift","starts_at":start,"ends_at":start+3600000,"employee_ids":[f.user],"nozzle_ids":["nozzle"]});
    let response = f.post(&owner, "/shifts", body.clone()).await;
    assert_eq!(response.status(), StatusCode::OK);
    let mut shift: Value = response.json().await.unwrap();
    assert!(!f.post(&owner, "/shifts", body).await.status().is_success());
    let path = format!("/shifts/{}/transition", shift["id"].as_str().unwrap());
    let opening = json!({"version":shift["version"],"to":"Open","readings":[{"nozzle_id":"nozzle","initial_ml":1000,"final_ml":null,"rollovers":0,"tiers":[]}]});
    let (one, two) = tokio::join!(
        f.post(&owner, &path, opening.clone()),
        f.post(&owner, &path, opening)
    );
    assert_ne!(one.status().is_success(), two.status().is_success());
    shift = if one.status().is_success() {
        one.json().await.unwrap()
    } else {
        two.json().await.unwrap()
    };
    let failed = f
        .post(
            &owner,
            &path,
            json!({"version":shift["version"],"to":"Closed","handover_note":"skip"}),
        )
        .await;
    assert!(!failed.status().is_success());
    let submitted=f.post(&owner,&path,json!({"version":shift["version"],"to":"PendingReview","readings":[{"nozzle_id":"nozzle","initial_ml":1000,"final_ml":11000,"rollovers":0,"tiers":[{"tier_id":"price","volume_ml":10000}]}],"settlements":[{"method":"card","amount_irr":300000,"reference":"POS-test"}]})).await;
    assert!(submitted.status().is_success());
    shift = submitted.json().await.unwrap();
    let mut reconciliation: Value = f
        .post(&owner, "/reconciliations", json!({"shift_id":shift["id"]}))
        .await
        .json()
        .await
        .unwrap();
    let reconcile_path = format!(
        "/reconciliations/{}/transition",
        reconciliation["id"].as_str().unwrap()
    );
    for phase in [
        "Calculating",
        "ReviewingDiscrepancies",
        "Approval",
        "Completed",
    ] {
        let response = f
            .post(
                &owner,
                &reconcile_path,
                json!({"version":reconciliation["version"],"to":phase,"investigation":"Checked"}),
            )
            .await;
        assert!(response.status().is_success(), "phase {phase}");
        reconciliation = response.json().await.unwrap();
    }
    assert_eq!(reconciliation["data"]["expected_irr"], 300000);
    assert_eq!(reconciliation["data"]["difference_irr"], 0);
    let inventory = f.get(&owner, "/inventory").await;
    assert_eq!(inventory["items"][0]["data"]["volume_ml"], 490000);
    let failed = f
        .post(
            &owner,
            &reconcile_path,
            json!({"version":reconciliation["version"],"to":"Completed"}),
        )
        .await;
    assert!(!failed.status().is_success());
    assert_eq!(
        f.get(&owner, "/inventory").await["items"][0]["data"]["volume_ml"],
        490000
    );
    let mut delivery:Value=f.post(&owner,"/deliveries",json!({"document_number":id(),"supplier":"Test","tank_id":"tank","fuel_type_id":"petrol","declared_ml":100000,"source":"simulated"})).await.json().await.unwrap();
    let delivery_path = format!(
        "/deliveries/{}/transition",
        delivery["id"].as_str().unwrap()
    );
    for phase in [
        "AwaitingMeasurements",
        "Measuring",
        "Reviewing",
        "Approved",
        "Completed",
    ] {
        let response=f.post(&owner,&delivery_path,json!({"version":delivery["version"],"to":phase,"before_ml":490000,"after_ml":590000,"note":"Verified"})).await;
        assert!(response.status().is_success());
        delivery = response.json().await.unwrap();
    }
    assert_eq!(
        f.get(&owner, "/inventory").await["items"][0]["data"]["volume_ml"],
        590000
    );
    let dashboard = f.get(&owner, "/dashboard?period=today").await;
    assert_eq!(dashboard["dispensed_ml"], 10000);
    assert_eq!(dashboard["inventory_ml"], 590000);
    let correction_path = format!(
        "/reconciliations/{}/corrections",
        reconciliation["id"].as_str().unwrap()
    );
    let correction = json!({"version":reconciliation["version"],"delta_irr":-100,"inventory_delta_ml":-1000,"tank_id":"tank","reason":"Verified late adjustment"});
    assert!(f
        .post(&owner, &correction_path, correction.clone())
        .await
        .status()
        .is_success());
    assert!(!f
        .post(&owner, &correction_path, correction)
        .await
        .status()
        .is_success());
    assert_eq!(
        f.get(&owner, "/inventory").await["items"][0]["data"]["volume_ml"],
        589000
    );
    let corrected_report = f.get(&owner, "/dashboard?period=today").await;
    assert_eq!(corrected_report["correction_irr"], -100);
    assert_eq!(
        corrected_report["financial_difference_irr"],
        dashboard["financial_difference_irr"]
    );
    let user = f
        .state
        .db
        .collection::<Document>("users")
        .find_one(doc! {"org_id":&f.org,"id":&f.user})
        .await
        .unwrap()
        .unwrap();
    assert!(user
        .get_document("data")
        .unwrap()
        .contains_key("password_hash"));
    let employees = f.get(&owner, "/employees").await;
    assert!(!employees.to_string().contains("password_hash"));
    let logout = f
        .client
        .post(format!("{}/auth/logout", f.base))
        .header("Origin", &f.origin)
        .send()
        .await
        .unwrap();
    assert_eq!(logout.status(), StatusCode::NO_CONTENT);
    let revoked = f
        .client
        .get(format!("{}/auth/me", f.base))
        .bearer_auth(owner)
        .send()
        .await
        .unwrap();
    assert_eq!(revoked.status(), StatusCode::UNAUTHORIZED);

    let isolated = Client::new();
    let login = isolated
        .post(format!("{}/auth/login", f.base))
        .header("Origin", &f.origin)
        .json(&json!({"organization":f.org,"email":"owner@test.invalid","password":f.password}))
        .send()
        .await
        .unwrap();
    assert_eq!(login.status(), StatusCode::OK);
    let old_cookie = login
        .headers()
        .get("set-cookie")
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    let rotated = isolated
        .post(format!("{}/auth/refresh", f.base))
        .header("Origin", &f.origin)
        .header("Cookie", &old_cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(rotated.status(), StatusCode::OK);
    let rotated_access = rotated.json::<Value>().await.unwrap()["access_token"]
        .as_str()
        .unwrap()
        .to_owned();
    let replay = isolated
        .post(format!("{}/auth/refresh", f.base))
        .header("Origin", &f.origin)
        .header("Cookie", &old_cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(replay.status(), StatusCode::UNAUTHORIZED);
    let revoked = isolated
        .get(format!("{}/auth/me", f.base))
        .bearer_auth(rotated_access)
        .send()
        .await
        .unwrap();
    assert_eq!(revoked.status(), StatusCode::UNAUTHORIZED);
}
