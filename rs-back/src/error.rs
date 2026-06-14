use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

#[derive(Debug)]
pub enum AppError {
    /// 400 with {"error": msg}
    BadRequest(String),
    /// 404 with {"error": "Not Found"}
    NotFound,
    /// 500 (logged); body {"error":"Internal Server Error"}
    Internal(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, msg) = match self {
            AppError::BadRequest(m) => (StatusCode::BAD_REQUEST, m),
            AppError::NotFound => (StatusCode::NOT_FOUND, "Not Found".to_string()),
            AppError::Internal(m) => {
                tracing::error!("internal error: {m}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal Server Error".to_string(),
                )
            }
        };
        (status, Json(json!({ "error": msg }))).into_response()
    }
}

/// Map std error types (e.g. deadpool/tokio-postgres errors) to a 500 so `?` works in handlers.
impl<E> From<E> for AppError
where
    E: std::error::Error,
{
    fn from(e: E) -> Self {
        AppError::Internal(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;

    #[test]
    fn not_found_is_404() {
        let resp = AppError::NotFound.into_response();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn bad_request_is_400() {
        let resp = AppError::BadRequest("x".into()).into_response();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }
}
