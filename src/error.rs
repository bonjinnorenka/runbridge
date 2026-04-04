//! エラー型の定義

use thiserror::Error;

/// アプリケーションのエラー型
#[derive(Error, Debug)]
pub enum Error {
    /// ルーティング文脈用の404。通常の route 不一致は router 層が直接 Response::not_found() を返す。
    #[error("Route not found: {0}")]
    RouteNotFound(String),

    /// ルート自体は存在するが、handler/service 層で対象リソースが存在しない
    #[error("Resource not found: {0}")]
    ResourceNotFound(String),

    /// 無効なリクエストボディ
    #[error("Invalid request body: {0}")]
    InvalidRequestBody(String),

    /// リソース状態や一意制約などの競合
    #[error("Conflict: {0}")]
    Conflict(String),

    /// Content-Type などメディア種別が不正
    #[error("Unsupported media type: {0}")]
    UnsupportedMediaType(String),

    /// 構文は正しいが意味的に妥当でない
    #[error("Validation error: {0}")]
    ValidationError(String),

    /// リクエストボディサイズが大きすぎる
    #[error("Request body too large: {0}")]
    PayloadTooLarge(String),

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

    /// レート制限
    #[error("Too many requests: {0}")]
    TooManyRequests(String),

    /// 一時的なサービス利用不可
    #[error("Service unavailable: {0}")]
    ServiceUnavailable(String),

    /// 上流のタイムアウト
    #[error("Gateway timeout: {0}")]
    GatewayTimeout(String),

    /// 未実装
    #[error("Not implemented: {0}")]
    NotImplemented(String),

    /// 無効なHTTPヘッダー
    #[error("Invalid header: {0}")]
    InvalidHeader(String),

    /// 無効なCookie
    #[error("Invalid cookie: {0}")]
    InvalidCookie(String),
}

impl Error {
    /// エラーからHTTPステータスコードを取得
    pub fn status_code(&self) -> u16 {
        match self {
            Error::RouteNotFound(_) => 404,
            Error::ResourceNotFound(_) => 404,
            Error::InvalidRequestBody(_) => 400,
            Error::Conflict(_) => 409,
            Error::UnsupportedMediaType(_) => 415,
            Error::ValidationError(_) => 422,
            Error::PayloadTooLarge(_) => 413,
            Error::ResponseSerializationError(_) => 500,
            Error::MiddlewareError(_) => 500,
            Error::InternalServerError(_) => 500,
            Error::ConfigurationError(_) => 500,
            Error::ExternalServiceError(_) => 502,
            Error::AuthenticationError(_) => 401,
            Error::AuthorizationError(_) => 403,
            Error::TooManyRequests(_) => 429,
            Error::ServiceUnavailable(_) => 503,
            Error::GatewayTimeout(_) => 504,
            Error::NotImplemented(_) => 501,
            Error::InvalidHeader(_) => 400,
            Error::InvalidCookie(_) => 400,
        }
    }
}
