use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};

/// Wrapper around `anyhow::Error` so handlers can return `anyhow::Result`
/// while still satisfying axum's `IntoResponse` requirement.
///
/// Carries an explicit status so client mistakes — a malformed upload, an
/// unknown system id — are not reported as server faults. Anything converted
/// via `?` defaults to 500; use the constructors to say otherwise.
pub struct AppError {
    status: StatusCode,
    error: anyhow::Error,
}

impl AppError {
    /// The request itself was wrong: unparseable body, failed validation.
    pub fn bad_request(error: impl Into<anyhow::Error>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            error: error.into(),
        }
    }

    /// The addressed resource does not exist.
    pub fn not_found(error: impl Into<anyhow::Error>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            error: error.into(),
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        (self.status, format!("{:#}", self.error)).into_response()
    }
}

impl<E> From<E> for AppError
where
    E: Into<anyhow::Error>,
{
    fn from(err: E) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            error: err.into(),
        }
    }
}
