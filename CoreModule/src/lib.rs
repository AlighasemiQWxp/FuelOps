pub mod database;
pub mod events;
pub mod http;
pub mod module;

pub use database::{Actor, AppState, Record, Transaction};
pub use http::{ApiError, ApiResult, Page, PageQuery};
pub use module::{CoreModule, Module};
pub use mongodb::bson;

pub fn now() -> i64 {
    chrono::Utc::now().timestamp_millis()
}
pub fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}

pub fn require(condition: bool, message: &str) -> ApiResult<()> {
    if condition {
        Ok(())
    } else {
        Err(ApiError::validation(message))
    }
}
