use serde::Serialize;

use crate::common::Response;
use crate::error::Error;

/// レスポンスへ変換する公開トレイト
pub trait IntoResponse {
    fn into_response(self) -> Response;
}

impl IntoResponse for Response {
    fn into_response(self) -> Response {
        self
    }
}

impl<T> IntoResponse for T
where
    T: Serialize,
{
    fn into_response(self) -> Response {
        match Response::ok().json(&self) {
            Ok(response) => response,
            Err(error) => Response::from_error(&error),
        }
    }
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        Response::from_error(&self)
    }
}
