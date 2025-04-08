//! エラー型の定義

use thiserror::Error;

/// アプリケーションのエラー型
#[derive(Error, Debug)]
pub enum Error {
    /// リクエストのルーティングエラー
    #[error("Route not found: {0}")]
    RouteNotFound(String),

    /// 無効なリクエストボディ
    #[error("Invalid request body: {0}")]
    InvalidRequestBody(String),

    /// レスポンスのシリアライズエラー
    #[error("Failed to serialize response: {0}")]
    ResponseSerializationError(String),

    /// ミドルウェアエラー
    #[error("Middleware error: {0}")]
    MiddlewareError(String),

    /// 内部サーバーエラー
    #[error("Internal server error: {0}")]
    InternalServerError(String),

    /// 設定エラー
    #[error("Configuration error: {0}")]
    ConfigurationError(String),

    /// 外部サービスエラー
    #[error("External service error: {0}")]
    ExternalServiceError(String),

    /// 認証エラー
    #[error("Authentication error: {0}")]
    AuthenticationError(String),

    /// 認可エラー
    #[error("Authorization error: {0}")]
    AuthorizationError(String),
}

impl Error {
    /// エラーからHTTPステータスコードを取得
    pub fn status_code(&self) -> u16 {
        match self {
            Error::RouteNotFound(_) => 404,
            Error::InvalidRequestBody(_) => 400,
            Error::ResponseSerializationError(_) => 500,
            Error::MiddlewareError(_) => 500,
            Error::InternalServerError(_) => 500,
            Error::ConfigurationError(_) => 500,
            Error::ExternalServiceError(_) => 502,
            Error::AuthenticationError(_) => 401,
            Error::AuthorizationError(_) => 403,
        }
    }
} 