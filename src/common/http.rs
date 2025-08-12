//! HTTP関連の基本型とユーティリティ

use std::collections::HashMap;
use std::fmt;
use std::io::Read;
use serde::{Serialize, Deserialize};
use flate2::read::GzDecoder;
use crate::error::Error;
use super::context::RequestContext;
use super::utils::{is_header_value_valid, get_max_body_size};

/// HTTPステータスコード
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusCode {
    // 2xx Success
    Ok = 200,
    Created = 201,
    NoContent = 204,
    
    // 4xx Client Error
    BadRequest = 400,
    Unauthorized = 401,
    Forbidden = 403,
    NotFound = 404,
    MethodNotAllowed = 405,
    Conflict = 409,
    UnprocessableEntity = 422,
    Locked = 423,
    TooManyRequests = 429,
    
    // 5xx Server Error
    InternalServerError = 500,
    NotImplemented = 501,
    BadGateway = 502,
    ServiceUnavailable = 503,
}

impl StatusCode {
    /// u16の値を取得
    pub fn as_u16(&self) -> u16 {
        *self as u16
    }

    /// 理由句を取得
    pub fn reason_phrase(&self) -> &'static str {
        match self {
            StatusCode::Ok => "OK",
            StatusCode::Created => "Created",
            StatusCode::NoContent => "No Content",
            StatusCode::BadRequest => "Bad Request",
            StatusCode::Unauthorized => "Unauthorized",
            StatusCode::Forbidden => "Forbidden",
            StatusCode::NotFound => "Not Found",
            StatusCode::MethodNotAllowed => "Method Not Allowed",
            StatusCode::Conflict => "Conflict",
            StatusCode::UnprocessableEntity => "Unprocessable Entity",
            StatusCode::Locked => "Locked",
            StatusCode::TooManyRequests => "Too Many Requests",
            StatusCode::InternalServerError => "Internal Server Error",
            StatusCode::NotImplemented => "Not Implemented",
            StatusCode::BadGateway => "Bad Gateway",
            StatusCode::ServiceUnavailable => "Service Unavailable",
        }
    }

    /// 成功ステータスかどうか判定
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.as_u16())
    }

    /// クライアントエラーかどうか判定
    pub fn is_client_error(&self) -> bool {
        (400..500).contains(&self.as_u16())
    }

    /// サーバーエラーかどうか判定
    pub fn is_server_error(&self) -> bool {
        (500..600).contains(&self.as_u16())
    }
}

impl From<StatusCode> for u16 {
    fn from(status: StatusCode) -> u16 {
        status.as_u16()
    }
}

/// HTTPメソッド
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Method {
    GET,
    POST,
    PUT,
    DELETE,
    PATCH,
    HEAD,
    OPTIONS,
}

impl fmt::Display for Method {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Method::GET => write!(f, "GET"),
            Method::POST => write!(f, "POST"),
            Method::PUT => write!(f, "PUT"),
            Method::DELETE => write!(f, "DELETE"),
            Method::PATCH => write!(f, "PATCH"),
            Method::HEAD => write!(f, "HEAD"),
            Method::OPTIONS => write!(f, "OPTIONS"),
        }
    }
}

impl Method {
    /// 文字列からMethodに変換
    pub fn from_str(method: &str) -> Option<Self> {
        match method.to_uppercase().as_str() {
            "GET" => Some(Method::GET),
            "POST" => Some(Method::POST),
            "PUT" => Some(Method::PUT),
            "DELETE" => Some(Method::DELETE),
            "PATCH" => Some(Method::PATCH),
            "HEAD" => Some(Method::HEAD),
            "OPTIONS" => Some(Method::OPTIONS),
            _ => None,
        }
    }
}

/// HTTPリクエスト
/// 注意：意図的にCloneトレイトを省略しています（RequestContextの安全性のため）
#[derive(Debug)]
pub struct Request {
    /// HTTPメソッド
    pub method: Method,
    /// リクエストパス
    pub path: String,
    /// クエリパラメータ
    pub query_params: HashMap<String, String>,
    /// HTTPヘッダー
    pub headers: HashMap<String, String>,
    /// リクエストボディ
    pub body: Option<Vec<u8>>,
    /// リクエストコンテキスト
    context: RequestContext,
}

impl Request {
    /// 新しいリクエストを作成
    pub fn new(method: Method, path: String) -> Self {
        Self {
            method,
            path,
            query_params: HashMap::new(),
            headers: HashMap::new(),
            body: None,
            context: RequestContext::new(),
        }
    }

    /// クエリパラメータを追加
    pub fn with_query_param(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.query_params.insert(key.into(), value.into());
        self
    }

    /// ヘッダーを追加（Requestではキーを小文字に正規化）
    pub fn with_header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        let k = key.into();
        let v = value.into();
        // 値の安全性チェック（CRLF/制御文字を拒否）
        if !is_header_value_valid(&v) {
            log::warn!("Request::with_header rejected invalid value for '{}': {:?}", k, v);
            return self;
        }
        // リクエスト側のヘッダーキーは大小無視のため小文字化して格納
        // Responseはこの型を使わないため影響なし
        let normalized_key = k.to_ascii_lowercase();
        self.headers.insert(normalized_key, v);
        self
    }

    /// ボディを追加
    pub fn with_body(mut self, body: Vec<u8>) -> Self {
        self.body = Some(body);
        self
    }

    /// ボディをJSONとしてパース
    pub fn json<T: for<'de> Deserialize<'de>>(&self) -> Result<T, Error> {
        if let Some(body) = &self.body {
            serde_json::from_slice(body)
                .map_err(|e| Error::InvalidRequestBody(e.to_string()))
        } else {
            Err(Error::InvalidRequestBody("No request body".to_string()))
        }
    }

    /// リクエストコンテキストの不変参照を取得
    pub fn context(&self) -> &RequestContext {
        &self.context
    }

    /// リクエストコンテキストの可変参照を取得
    pub fn context_mut(&mut self) -> &mut RequestContext {
        &mut self.context
    }

    /// リクエストコンテキストを設定
    pub fn with_context(mut self, context: RequestContext) -> Self {
        self.context = context;
        self
    }

    /// コンテキストを除外してリクエストをクローン（安全なデータ複製）
    /// コンテキストは意図的に新しい空の状態で初期化されます
    pub fn clone_without_context(&self) -> Self {
        #[cfg(debug_assertions)]
        log::debug!("Request::clone_without_context() called - context will be empty");
        
        Self {
            method: self.method,
            path: self.path.clone(),
            query_params: self.query_params.clone(),
            headers: self.headers.clone(),
            body: self.body.clone(),
            context: RequestContext::new(),
        }
    }

    /// リクエストボディがgzipエンコードされている場合は解凍する
    /// Content-Encodingヘッダーをチェックし、gzipの場合のみ処理を実行
    /// 解凍後のサイズが上限を超える場合はPayloadTooLargeエラーを返す
    pub fn decompress_gzip_body(&mut self) -> Result<(), Error> {
        // Content-Encodingヘッダーをチェック（小文字で正規化済み）
        if let Some(encoding) = self.headers.get("content-encoding") {
            if encoding.to_lowercase() == "gzip" {
                if let Some(body_data) = &self.body {
                    let max_body_size = get_max_body_size();
                    let mut decoder = GzDecoder::new(&body_data[..]);
                    let mut decompressed = Vec::new();
                    let mut buffer = [0u8; 8192]; // 8KBチャンクで読み込み
                    
                    loop {
                        match decoder.read(&mut buffer) {
                            Ok(0) => break, // EOF
                            Ok(n) => {
                                // 新しいデータを追加する前にサイズをチェック
                                if decompressed.len() + n > max_body_size {
                                    log::warn!(
                                        "Decompressed gzip body too large: {} + {} > {} bytes",
                                        decompressed.len(),
                                        n,
                                        max_body_size
                                    );
                                    return Err(Error::PayloadTooLarge(format!(
                                        "Decompressed body too large (>{} bytes)",
                                        max_body_size
                                    )));
                                }
                                decompressed.extend_from_slice(&buffer[..n]);
                            }
                            Err(e) => {
                                log::warn!("Failed to decompress gzip body: {}", e);
                                return Err(Error::InvalidRequestBody(
                                    format!("Invalid gzip-encoded request body: {}", e)
                                ));
                            }
                        }
                    }
                    
                    // 解凍成功：ボディを更新し、Content-Encodingヘッダーを削除
                    self.body = Some(decompressed);
                    self.headers.remove("content-encoding");
                    log::debug!("Successfully decompressed gzip request body");
                }
            }
        }
        Ok(())
    }
}

/// HTTPレスポンス
#[derive(Debug, Clone)]
pub struct Response {
    /// HTTPステータスコード
    pub status: u16,
    /// HTTPヘッダー
    pub headers: HashMap<String, String>,
    /// レスポンスボディ
    pub body: Option<Vec<u8>>,
}

impl Response {
    /// 新しいレスポンスを作成
    pub fn new(status: u16) -> Self {
        let mut headers = HashMap::new();
        // 既定のセキュリティヘッダーを注入（未設定の場合のみ）
        inject_default_security_headers(&mut headers);
        Self {
            status,
            headers,
            body: None,
        }
    }

    /// StatusCodeから新しいレスポンスを作成
    pub fn with_status(status: StatusCode) -> Self {
        let mut headers = HashMap::new();
        // 既定のセキュリティヘッダーを注入（未設定の場合のみ）
        inject_default_security_headers(&mut headers);
        Self {
            status: status.as_u16(),
            headers,
            body: None,
        }
    }

    /// ヘッダーを追加
    pub fn with_header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        let k = key.into();
        let v = value.into();
        if !is_header_value_valid(&v) {
            log::warn!("Response::with_header rejected invalid value for '{}': {:?}", k, v);
            return self;
        }
        self.headers.insert(k, v);
        self
    }

    /// ボディを追加
    pub fn with_body(mut self, body: Vec<u8>) -> Self {
        self.body = Some(body);
        self
    }

    /// JSONをボディとして設定
    pub fn json<T: Serialize>(mut self, value: &T) -> Result<Self, Error> {
        let json = serde_json::to_vec(value)
            .map_err(|e| Error::ResponseSerializationError(e.to_string()))?;
        
        self.headers.insert("Content-Type".to_string(), "application/json".to_string());
        self.body = Some(json);
        Ok(self)
    }

    /// 200 OKレスポンスを作成
    pub fn ok() -> Self {
        Self::new(200)
    }

    /// 201 Createdレスポンスを作成
    pub fn created() -> Self {
        Self::new(201)
    }

    /// 204 No Contentレスポンスを作成
    pub fn no_content() -> Self {
        Self::new(204)
    }

    /// 400 Bad Requestレスポンスを作成
    pub fn bad_request() -> Self {
        Self::new(400)
    }

    /// 401 Unauthorizedレスポンスを作成
    pub fn unauthorized() -> Self {
        Self::new(401)
    }

    /// 403 Forbiddenレスポンスを作成
    pub fn forbidden() -> Self {
        Self::new(403)
    }

    /// 404 Not Foundレスポンスを作成
    pub fn not_found() -> Self {
        Self::new(404)
    }

    /// 500 Internal Server Errorレスポンスを作成
    pub fn internal_server_error() -> Self {
        Self::new(500)
    }

    /// Error型から固定メッセージのレスポンスを生成
    pub fn from_error(error: &crate::error::Error) -> Self {
        let status = error.status_code();
        let message = match status {
            400 => "Bad Request",
            401 => "Unauthorized",
            403 => "Forbidden",
            404 => "Not Found",
            413 => "Payload Too Large",
            500 | 502 => "Internal Server Error",
            _ => "Error",
        };
        Response::new(status)
            .with_header("Content-Type", "text/plain")
            .with_body(message.as_bytes().to_vec())
    }
}

/// レスポンス構築のためのビルダー
#[derive(Debug, Clone)]
pub struct ResponseBuilder {
    status: u16,
    headers: HashMap<String, String>,
    body: Option<Vec<u8>>,
}

impl ResponseBuilder {
    /// 新しいResponseBuilderを作成（u16ステータスコード）
    pub fn new(status: u16) -> Self {
        let mut headers = HashMap::new();
        // 既定のセキュリティヘッダーを注入（未設定の場合のみ）
        inject_default_security_headers(&mut headers);
        Self { status, headers, body: None }
    }

    /// 新しいResponseBuilderを作成（StatusCode）
    pub fn with_status(status: StatusCode) -> Self {
        let mut headers = HashMap::new();
        // 既定のセキュリティヘッダーを注入（未設定の場合のみ）
        inject_default_security_headers(&mut headers);
        Self { status: status.as_u16(), headers, body: None }
    }

    /// 既存のResponseからResponseBuilderを作成
    pub fn from(response: Response) -> Self {
        Self {
            status: response.status,
            headers: response.headers,
            body: response.body,
        }
    }

    /// ヘッダーを追加
    pub fn header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        let k = key.into();
        let v = value.into();
        if !is_header_value_valid(&v) {
            log::warn!("ResponseBuilder::header rejected invalid value for '{}': {:?}", k, v);
            return self;
        }
        self.headers.insert(k, v);
        self
    }

    /// 複数のヘッダーを一括追加
    pub fn headers(mut self, headers: HashMap<String, String>) -> Self {
        self.headers.extend(headers);
        self
    }

    /// 標準的なセキュリティヘッダーを一括追加
    pub fn security_headers(mut self) -> Self {
        self.headers.insert("X-Content-Type-Options".to_string(), "nosniff".to_string());
        self.headers.insert("X-Frame-Options".to_string(), "DENY".to_string());
        self.headers.insert("X-XSS-Protection".to_string(), "1; mode=block".to_string());
        self.headers.insert("Referrer-Policy".to_string(), "strict-origin-when-cross-origin".to_string());
        self.headers.insert("Content-Security-Policy".to_string(), "default-src 'self'".to_string());
        self
    }

    /// JSONボディを設定
    pub fn json<T: Serialize>(mut self, data: &T) -> Result<Self, Error> {
        let json = serde_json::to_vec(data)
            .map_err(|e| Error::ResponseSerializationError(e.to_string()))?;
        
        self.headers.insert("Content-Type".to_string(), "application/json".to_string());
        self.body = Some(json);
        Ok(self)
    }

    /// ボディを設定
    pub fn body(mut self, body: Vec<u8>) -> Self {
        self.body = Some(body);
        self
    }

    /// テキストボディを設定
    pub fn text(mut self, text: impl Into<String>) -> Self {
        let text = text.into();
        self.headers.insert("Content-Type".to_string(), "text/plain; charset=utf-8".to_string());
        self.body = Some(text.into_bytes());
        self
    }

    /// HTMLボディを設定
    pub fn html(mut self, html: impl Into<String>) -> Self {
        let html = html.into();
        self.headers.insert("Content-Type".to_string(), "text/html; charset=utf-8".to_string());
        self.body = Some(html.into_bytes());
        self
    }

    /// Responseを構築
    pub fn build(mut self) -> Response {
        // build時にも不足があればセキュリティヘッダーを補完
        inject_default_security_headers(&mut self.headers);
        Response { status: self.status, headers: self.headers, body: self.body }
    }
}

/// 既定のセキュリティヘッダーを不足時に注入する
fn inject_default_security_headers(map: &mut HashMap<String, String>) {
    // ユーザーが上書きしたい場合を尊重し、未設定時のみ入れる
    map.entry("X-Content-Type-Options".to_string())
        .or_insert_with(|| "nosniff".to_string());
    map.entry("X-Frame-Options".to_string())
        .or_insert_with(|| "DENY".to_string());
    map.entry("X-XSS-Protection".to_string())
        .or_insert_with(|| "1; mode=block".to_string());
    map.entry("Referrer-Policy".to_string())
        .or_insert_with(|| "strict-origin-when-cross-origin".to_string());
    map.entry("Content-Security-Policy".to_string())
        .or_insert_with(|| "default-src 'self'".to_string());
}
