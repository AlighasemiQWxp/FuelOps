use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};

pub type ApiResult<T> = Result<T, ApiError>;

#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub struct ApiError {
    pub status: StatusCode,
    pub message: String,
}
impl ApiError {
    pub fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }
    pub fn validation(message: impl Into<String>) -> Self {
        Self::new(StatusCode::UNPROCESSABLE_ENTITY, message)
    }
    pub fn forbidden() -> Self {
        Self::new(StatusCode::FORBIDDEN, "Access denied")
    }
    pub fn unauthorized() -> Self {
        Self::new(StatusCode::UNAUTHORIZED, "Authentication required")
    }
    pub fn conflict() -> Self {
        Self::new(
            StatusCode::CONFLICT,
            "Record changed or operation conflicts; refresh and retry",
        )
    }
    pub fn missing() -> Self {
        Self::new(StatusCode::NOT_FOUND, "Record not found")
    }
}
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(serde_json::json!({"error": self.message})),
        )
            .into_response()
    }
}
impl From<mongodb::error::Error> for ApiError {
    fn from(error: mongodb::error::Error) -> Self {
        if error.contains_label("TransientTransactionError") || error.to_string().contains("E11000")
        {
            return Self::conflict();
        }
        Self::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "Database operation unavailable; refresh before retrying",
        )
    }
}
impl From<mongodb::bson::ser::Error> for ApiError {
    fn from(_: mongodb::bson::ser::Error) -> Self {
        Self::validation("Invalid record")
    }
}
impl From<mongodb::bson::de::Error> for ApiError {
    fn from(_: mongodb::bson::de::Error) -> Self {
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Stored record is invalid",
        )
    }
}
#[derive(Default, Deserialize)]
pub struct PageQuery {
    pub page: Option<u64>,
    pub limit: Option<i64>,
    pub status: Option<String>,
}
impl PageQuery {
    pub fn limit(&self) -> i64 {
        self.limit.unwrap_or(30).clamp(1, 100)
    }
    pub fn skip(&self) -> u64 {
        self.page.unwrap_or(1).clamp(1, 10000).saturating_sub(1) * self.limit() as u64
    }
}
#[derive(Serialize)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub total: u64,
}
