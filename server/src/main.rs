use axum::{
    extract::{DefaultBodyLimit, State},
    http::{header, HeaderValue, Method},
    middleware,
    routing::get,
    Json, Router,
};
use fuel_core::{database::Config, ApiResult, AppState, CoreModule};
use std::net::SocketAddr;
use tower_http::{cors::CorsLayer, set_header::SetResponseHeaderLayer};

fn modules() -> CoreModule {
    CoreModule::new()
        .register(fuel_permission::PermissionModule)
        .register(fuel_auth::AuthModule)
        .register(fuel_station::StationModule)
        .register(fuel_employee::EmployeeModule)
        .register(fuel_shift::ShiftModule)
        .register(fuel_inventory::InventoryModule)
        .register(fuel_reconciliation::ReconciliationModule)
        .register(fuel_delivery::DeliveryModule)
        .register(fuel_alert::AlertModule)
        .register(fuel_dashboard::DashboardModule)
        .register(fuel_audit::AuditModule)
}
async fn health(State(state): State<AppState>) -> ApiResult<Json<serde_json::Value>> {
    state
        .db
        .run_command(fuel_core::bson::doc! {"ping":1})
        .await?;
    Ok(Json(
        serde_json::json!({"status":"ok","application":"Fuel Ops"}),
    ))
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();
    let state = AppState::connect(Config::from_env()?).await?;
    let protected =
        modules()
            .initialize(&state)
            .await?
            .route_layer(middleware::from_fn_with_state(
                state.clone(),
                fuel_auth::authenticate,
            ));
    if std::env::args().any(|argument| argument == "--migrate") {
        CoreModule::shutdown(state).await;
        return Ok(());
    }
    let cors = CorsLayer::new()
        .allow_origin(state.config.web_origin.parse::<HeaderValue>()?)
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::OPTIONS])
        .allow_headers([
            header::CONTENT_TYPE,
            header::AUTHORIZATION,
            header::HeaderName::from_static("x-station-id"),
        ])
        .allow_credentials(true);
    let app = Router::new()
        .route("/health", get(health))
        .nest("/api/v1", protected.merge(fuel_auth::public_routes()))
        .layer(DefaultBodyLimit::max(128 * 1024))
        .layer(cors)
        .layer(SetResponseHeaderLayer::if_not_present(
            header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            header::CACHE_CONTROL,
            HeaderValue::from_static("no-store"),
        ))
        .with_state(state.clone());
    let (stop, mut stopped) = tokio::sync::watch::channel(false);
    let event_state = state.clone();
    let mut events = state.events.subscribe();
    let worker = tokio::spawn(async move {
        let mut timer = tokio::time::interval(std::time::Duration::from_secs(15));
        loop {
            tokio::select! { _=timer.tick()=>{}, _=events.recv()=>{}, _=stopped.changed()=>{break;} }
            if let Err(error) = fuel_alert::process_events(&event_state).await {
                let _=event_state.db.collection::<fuel_core::bson::Document>("worker_health").update_one(fuel_core::bson::doc! {"_id":"alerts"},fuel_core::bson::doc! {"$set":{"last_failure":fuel_core::now(),"status":error.status.as_u16() as i32}}).upsert(true).await;
            }
        }
    });
    let address = std::env::var("BIND_ADDRESS").unwrap_or_else(|_| "127.0.0.1:8080".into());
    let listener = tokio::net::TcpListener::bind(address).await?;
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;
    let _ = stop.send(true);
    let _ = worker.await;
    CoreModule::shutdown(state).await;
    Ok(())
}
async fn shutdown_signal() {
    #[cfg(unix)]
    {
        if let Ok(mut terminate) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            tokio::select! {_=tokio::signal::ctrl_c()=>{},_=terminate.recv()=>{}}
            return;
        }
    }
    let _ = tokio::signal::ctrl_c().await;
}
