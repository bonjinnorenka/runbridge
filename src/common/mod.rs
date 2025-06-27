//! 共通の抽象化レイヤーとトレイト定義

use std::collections::HashMap;
use std::fmt;
use std::any::Any;
use std::time::Duration;
use async_trait::async_trait;
use serde::{Serialize, Deserialize};
use chrono::{DateTime, Utc};
use crate::error::Error;

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

/// リクエストコンテキスト（ミドルウェア間でのデータ共有）
#[derive(Debug, Default)]
pub struct RequestContext {
    metadata: HashMap<String, Box<dyn Any + Send + Sync>>,
}

impl RequestContext {
    /// 新しいRequestContextを作成
    pub fn new() -> Self {
        Self {
            metadata: HashMap::new(),
        }
    }

    /// 値を設定
    pub fn set<T: Send + Sync + 'static>(&mut self, key: &str, value: T) {
        self.metadata.insert(key.to_string(), Box::new(value));
    }

    /// 値を取得
    pub fn get<T: 'static>(&self, key: &str) -> Option<&T> {
        self.metadata
            .get(key)
            .and_then(|boxed| boxed.downcast_ref::<T>())
    }

    /// 値を削除して返却
    pub fn remove<T: 'static>(&mut self, key: &str) -> Option<T> {
        self.metadata
            .remove(key)
            .and_then(|boxed| boxed.downcast::<T>().ok())
            .map(|boxed| *boxed)
    }

    /// 指定されたキーが存在するかチェック
    pub fn contains_key(&self, key: &str) -> bool {
        self.metadata.contains_key(key)
    }

    /// 全てのキーを取得
    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.metadata.keys()
    }

    /// コンテキストをクリア
    pub fn clear(&mut self) {
        self.metadata.clear();
    }

    /// コンテキストが空かどうか
    pub fn is_empty(&self) -> bool {
        self.metadata.is_empty()
    }
}

impl Clone for RequestContext {
    fn clone(&self) -> Self {
        // Anyトレイトはcloneをサポートしていないため、新しい空のコンテキストを作成
        Self::new()
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
#[derive(Debug, Clone)]
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

    /// ヘッダーを追加
    pub fn with_header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(key.into(), value.into());
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
        Self {
            status,
            headers: HashMap::new(),
            body: None,
        }
    }

    /// StatusCodeから新しいレスポンスを作成
    pub fn with_status(status: StatusCode) -> Self {
        Self {
            status: status.as_u16(),
            headers: HashMap::new(),
            body: None,
        }
    }

    /// ヘッダーを追加
    pub fn with_header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(key.into(), value.into());
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
        Self {
            status,
            headers: HashMap::new(),
            body: None,
        }
    }

    /// 新しいResponseBuilderを作成（StatusCode）
    pub fn with_status(status: StatusCode) -> Self {
        Self {
            status: status.as_u16(),
            headers: HashMap::new(),
            body: None,
        }
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
        self.headers.insert(key.into(), value.into());
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
    pub fn build(self) -> Response {
        Response {
            status: self.status,
            headers: self.headers,
            body: self.body,
        }
    }
}

/// ハンドラーの特性
#[async_trait]
pub trait Handler: Send + Sync {
    /// パスとメソッドがこのハンドラにマッチするかどうかを判定
    fn matches(&self, path: &str, method: &Method) -> bool;
    
    /// ハンドラに関連付けられたパスパターン文字列を取得
    fn path_pattern(&self) -> &str;

    /// リクエストを処理
    async fn handle(&self, req: Request) -> Result<Response, Error>;
}

/// ミドルウェアの特性
#[async_trait]
pub trait Middleware: Send + Sync {
    /// リクエスト前の処理
    async fn pre_process(&self, req: Request) -> Result<Request, Error>;
    
    /// レスポンス後の処理
    async fn post_process(&self, res: Response) -> Result<Response, Error>;
}

/// SameSite属性
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SameSite {
    Strict,
    Lax,
    None,
}

impl fmt::Display for SameSite {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SameSite::Strict => write!(f, "Strict"),
            SameSite::Lax => write!(f, "Lax"),
            SameSite::None => write!(f, "None"),
        }
    }
}

/// HTTPクッキー
#[derive(Debug, Clone)]
pub struct Cookie {
    pub name: String,
    pub value: String,
    pub path: Option<String>,
    pub domain: Option<String>,
    pub expires: Option<DateTime<Utc>>,
    pub max_age: Option<Duration>,
    pub secure: bool,
    pub http_only: bool,
    pub same_site: Option<SameSite>,
}

impl Cookie {
    /// 新しいクッキーを作成
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
            path: None,
            domain: None,
            expires: None,
            max_age: None,
            secure: false,
            http_only: false,
            same_site: None,
        }
    }

    /// パスを設定
    pub fn with_path(mut self, path: impl Into<String>) -> Self {
        self.path = Some(path.into());
        self
    }

    /// ドメインを設定
    pub fn with_domain(mut self, domain: impl Into<String>) -> Self {
        self.domain = Some(domain.into());
        self
    }

    /// 有効期限を設定
    pub fn with_expires(mut self, expires: DateTime<Utc>) -> Self {
        self.expires = Some(expires);
        self
    }

    /// 最大年齢を設定
    pub fn with_max_age(mut self, max_age: Duration) -> Self {
        self.max_age = Some(max_age);
        self
    }

    /// セキュアフラグを設定
    pub fn secure(mut self, secure: bool) -> Self {
        self.secure = secure;
        self
    }

    /// HttpOnlyフラグを設定
    pub fn http_only(mut self, http_only: bool) -> Self {
        self.http_only = http_only;
        self
    }

    /// SameSite属性を設定
    pub fn with_same_site(mut self, same_site: SameSite) -> Self {
        self.same_site = Some(same_site);
        self
    }

    /// Set-Cookieヘッダー値を生成
    pub fn to_header_value(&self) -> String {
        let mut cookie_str = format!("{}={}", self.name, self.value);

        if let Some(path) = &self.path {
            cookie_str.push_str(&format!("; Path={}", path));
        }

        if let Some(domain) = &self.domain {
            cookie_str.push_str(&format!("; Domain={}", domain));
        }

        if let Some(expires) = &self.expires {
            cookie_str.push_str(&format!("; Expires={}", expires.format("%a, %d %b %Y %H:%M:%S GMT")));
        }

        if let Some(max_age) = &self.max_age {
            cookie_str.push_str(&format!("; Max-Age={}", max_age.as_secs()));
        }

        if self.secure {
            cookie_str.push_str("; Secure");
        }

        if self.http_only {
            cookie_str.push_str("; HttpOnly");
        }

        if let Some(same_site) = &self.same_site {
            cookie_str.push_str(&format!("; SameSite={}", same_site));
        }

        cookie_str
    }
}

/// CGI環境向けのユーティリティ機能
#[cfg(feature = "cgi")]
pub mod cgi_utils {
    use super::*;
    use std::env;

    /// 環境変数を取得
    pub fn extract_env_var(key: &str) -> Option<String> {
        env::var(key).ok()
    }

    /// CGI環境からクッキーを抽出
    pub fn extract_cookies() -> HashMap<String, String> {
        let mut cookies = HashMap::new();
        
        if let Some(cookie_header) = extract_env_var("HTTP_COOKIE") {
            for cookie_pair in cookie_header.split(';') {
                let parts: Vec<&str> = cookie_pair.trim().splitn(2, '=').collect();
                if parts.len() == 2 {
                    cookies.insert(parts[0].to_string(), parts[1].to_string());
                }
            }
        }
        
        cookies
    }

    /// CGI環境からHTTPヘッダーを抽出
    pub fn extract_headers() -> HashMap<String, String> {
        let mut headers = HashMap::new();
        
        for (key, value) in env::vars() {
            if key.starts_with("HTTP_") {
                let header_name = key[5..].replace('_', "-").to_lowercase();
                headers.insert(header_name, value);
            }
        }
        
        // 特別なヘッダーを個別に処理
        if let Some(content_type) = extract_env_var("CONTENT_TYPE") {
            headers.insert("content-type".to_string(), content_type);
        }
        
        if let Some(content_length) = extract_env_var("CONTENT_LENGTH") {
            headers.insert("content-length".to_string(), content_length);
        }
        
        headers
    }

    /// レスポンスにクッキーを設定
    pub fn set_cookie(response: &mut Response, cookie: Cookie) {
        response.headers.insert("Set-Cookie".to_string(), cookie.to_header_value());
    }

    /// レスポンスに複数のクッキーを設定
    pub fn set_cookies(response: &mut Response, cookies: Vec<Cookie>) {
        for cookie in cookies {
            // Set-Cookieヘッダーは複数設定可能だが、HashMapでは上書きされるため
            // 既存の実装では最後のクッキーのみが有効になる
            // 実際の実装では Vec<(String, String)> を使用するか、
            // 複数のSet-Cookieヘッダーを連結する必要がある
            set_cookie(response, cookie);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Serialize, Deserialize};

    #[test]
    fn test_method_from_str() {
        assert_eq!(Method::from_str("GET"), Some(Method::GET));
        assert_eq!(Method::from_str("get"), Some(Method::GET));
        assert_eq!(Method::from_str("POST"), Some(Method::POST));
        assert_eq!(Method::from_str("PUT"), Some(Method::PUT));
        assert_eq!(Method::from_str("DELETE"), Some(Method::DELETE));
        assert_eq!(Method::from_str("PATCH"), Some(Method::PATCH));
        assert_eq!(Method::from_str("HEAD"), Some(Method::HEAD));
        assert_eq!(Method::from_str("OPTIONS"), Some(Method::OPTIONS));
        assert_eq!(Method::from_str("INVALID"), None);
    }

    #[test]
    fn test_request_builder() {
        let req = Request::new(Method::GET, "/test".to_string())
            .with_query_param("key1", "value1")
            .with_query_param("key2", "value2")
            .with_header("Content-Type", "application/json")
            .with_body(b"test body".to_vec());

        assert_eq!(req.method, Method::GET);
        assert_eq!(req.path, "/test");
        assert_eq!(req.query_params.get("key1"), Some(&"value1".to_string()));
        assert_eq!(req.query_params.get("key2"), Some(&"value2".to_string()));
        assert_eq!(req.headers.get("Content-Type"), Some(&"application/json".to_string()));
        assert_eq!(req.body.as_ref().unwrap(), &b"test body".to_vec());
    }

    #[test]
    fn test_response_builder() {
        let res = Response::ok()
            .with_header("Content-Type", "text/plain")
            .with_body(b"Hello, world!".to_vec());

        assert_eq!(res.status, 200);
        assert_eq!(res.headers.get("Content-Type"), Some(&"text/plain".to_string()));
        assert_eq!(res.body.as_ref().unwrap(), &b"Hello, world!".to_vec());
    }

    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct TestData {
        name: String,
        value: i32,
    }

    #[test]
    fn test_response_json() {
        let test_data = TestData {
            name: "test".to_string(),
            value: 42,
        };

        let res = Response::ok().json(&test_data).unwrap();

        assert_eq!(res.status, 200);
        assert_eq!(res.headers.get("Content-Type"), Some(&"application/json".to_string()));
        
        // ボディをJSONとしてデコード
        let body_str = String::from_utf8(res.body.unwrap()).unwrap();
        let decoded: TestData = serde_json::from_str(&body_str).unwrap();
        
        assert_eq!(decoded, test_data);
    }

    #[test]
    fn test_request_json() {
        let test_data = TestData {
            name: "test".to_string(),
            value: 42,
        };

        // JSONデータを含むリクエストを作成
        let json_bytes = serde_json::to_vec(&test_data).unwrap();
        let req = Request::new(Method::POST, "/test".to_string())
            .with_header("Content-Type", "application/json")
            .with_body(json_bytes);

        // JSONデータを取得
        let parsed: TestData = req.json().unwrap();
        
        assert_eq!(parsed, test_data);
    }

    #[test]
    fn test_status_code() {
        // 基本的な値のテスト
        assert_eq!(StatusCode::Ok.as_u16(), 200);
        assert_eq!(StatusCode::Created.as_u16(), 201);
        assert_eq!(StatusCode::BadRequest.as_u16(), 400);
        assert_eq!(StatusCode::Unauthorized.as_u16(), 401);
        assert_eq!(StatusCode::InternalServerError.as_u16(), 500);

        // 理由句のテスト
        assert_eq!(StatusCode::Ok.reason_phrase(), "OK");
        assert_eq!(StatusCode::NotFound.reason_phrase(), "Not Found");
        assert_eq!(StatusCode::InternalServerError.reason_phrase(), "Internal Server Error");

        // カテゴリ判定のテスト
        assert!(StatusCode::Ok.is_success());
        assert!(StatusCode::Created.is_success());
        assert!(!StatusCode::BadRequest.is_success());
        assert!(!StatusCode::InternalServerError.is_success());

        assert!(StatusCode::BadRequest.is_client_error());
        assert!(StatusCode::NotFound.is_client_error());
        assert!(!StatusCode::Ok.is_client_error());
        assert!(!StatusCode::InternalServerError.is_client_error());

        assert!(StatusCode::InternalServerError.is_server_error());
        assert!(StatusCode::BadGateway.is_server_error());
        assert!(!StatusCode::Ok.is_server_error());
        assert!(!StatusCode::BadRequest.is_server_error());
    }

    #[test]
    fn test_response_with_status_code() {
        let response = Response::with_status(StatusCode::Ok);
        assert_eq!(response.status, 200);

        let response = Response::with_status(StatusCode::NotFound);
        assert_eq!(response.status, 404);

        let response = Response::with_status(StatusCode::InternalServerError);
        assert_eq!(response.status, 500);
    }

    #[test]
    fn test_status_code_from_trait() {
        let status_u16: u16 = StatusCode::Ok.into();
        assert_eq!(status_u16, 200);

        let status_u16: u16 = StatusCode::BadRequest.into();
        assert_eq!(status_u16, 400);
    }

    #[test]
    fn test_response_builder_basic() {
        let response = ResponseBuilder::new(200)
            .header("Content-Type", "application/json")
            .header("X-Custom-Header", "custom-value")
            .body(b"test body".to_vec())
            .build();

        assert_eq!(response.status, 200);
        assert_eq!(response.headers.get("Content-Type"), Some(&"application/json".to_string()));
        assert_eq!(response.headers.get("X-Custom-Header"), Some(&"custom-value".to_string()));
        assert_eq!(response.body.as_ref().unwrap(), &b"test body".to_vec());
    }

    #[test]
    fn test_response_builder_with_status_code() {
        let response = ResponseBuilder::with_status(StatusCode::Created)
            .text("Created successfully")
            .build();

        assert_eq!(response.status, 201);
        assert_eq!(response.headers.get("Content-Type"), Some(&"text/plain; charset=utf-8".to_string()));
        assert_eq!(response.body.as_ref().unwrap(), &b"Created successfully".to_vec());
    }

    #[test]
    fn test_response_builder_json() {
        let test_data = TestData {
            name: "test".to_string(),
            value: 42,
        };

        let response = ResponseBuilder::new(200)
            .json(&test_data)
            .unwrap()
            .build();

        assert_eq!(response.status, 200);
        assert_eq!(response.headers.get("Content-Type"), Some(&"application/json".to_string()));
        
        // ボディをJSONとしてデコード
        let body_str = String::from_utf8(response.body.unwrap()).unwrap();
        let decoded: TestData = serde_json::from_str(&body_str).unwrap();
        assert_eq!(decoded, test_data);
    }

    #[test]
    fn test_response_builder_security_headers() {
        let response = ResponseBuilder::new(200)
            .security_headers()
            .text("Secure response")
            .build();

        assert_eq!(response.status, 200);
        assert_eq!(response.headers.get("X-Content-Type-Options"), Some(&"nosniff".to_string()));
        assert_eq!(response.headers.get("X-Frame-Options"), Some(&"DENY".to_string()));
        assert_eq!(response.headers.get("X-XSS-Protection"), Some(&"1; mode=block".to_string()));
        assert_eq!(response.headers.get("Referrer-Policy"), Some(&"strict-origin-when-cross-origin".to_string()));
        assert_eq!(response.headers.get("Content-Security-Policy"), Some(&"default-src 'self'".to_string()));
        assert_eq!(response.headers.get("Content-Type"), Some(&"text/plain; charset=utf-8".to_string()));
    }

    #[test]
    fn test_response_builder_html() {
        let html_content = "<html><body>Hello World</body></html>";
        let response = ResponseBuilder::new(200)
            .html(html_content)
            .build();

        assert_eq!(response.status, 200);
        assert_eq!(response.headers.get("Content-Type"), Some(&"text/html; charset=utf-8".to_string()));
        assert_eq!(response.body.as_ref().unwrap(), &html_content.as_bytes().to_vec());
    }

    #[test]
    fn test_response_builder_from_response() {
        let original = Response::new(404)
            .with_header("Original-Header", "original-value")
            .with_body(b"original body".to_vec());

        let modified = ResponseBuilder::from(original)
            .header("Additional-Header", "additional-value")
            .build();

        assert_eq!(modified.status, 404);
        assert_eq!(modified.headers.get("Original-Header"), Some(&"original-value".to_string()));
        assert_eq!(modified.headers.get("Additional-Header"), Some(&"additional-value".to_string()));
        assert_eq!(modified.body.as_ref().unwrap(), &b"original body".to_vec());
    }

    #[test]
    fn test_response_builder_multiple_headers() {
        let mut headers = HashMap::new();
        headers.insert("Header1".to_string(), "Value1".to_string());
        headers.insert("Header2".to_string(), "Value2".to_string());

        let response = ResponseBuilder::new(200)
            .header("Header0", "Value0")
            .headers(headers)
            .header("Header3", "Value3")
            .build();

        assert_eq!(response.headers.get("Header0"), Some(&"Value0".to_string()));
        assert_eq!(response.headers.get("Header1"), Some(&"Value1".to_string()));
        assert_eq!(response.headers.get("Header2"), Some(&"Value2".to_string()));
        assert_eq!(response.headers.get("Header3"), Some(&"Value3".to_string()));
    }

    #[test]
    fn test_request_context_basic() {
        let mut context = RequestContext::new();

        // 値の設定と取得
        context.set("string_val", "hello".to_string());
        context.set("int_val", 42i32);
        context.set("bool_val", true);

        assert_eq!(context.get::<String>("string_val"), Some(&"hello".to_string()));
        assert_eq!(context.get::<i32>("int_val"), Some(&42));
        assert_eq!(context.get::<bool>("bool_val"), Some(&true));

        // 存在しないキー
        assert_eq!(context.get::<String>("nonexistent"), None);

        // 間違った型
        assert_eq!(context.get::<i32>("string_val"), None);
    }

    #[test]
    fn test_request_context_contains_and_keys() {
        let mut context = RequestContext::new();
        
        assert!(context.is_empty());
        assert!(!context.contains_key("test"));

        context.set("key1", "value1".to_string());
        context.set("key2", 123);

        assert!(!context.is_empty());
        assert!(context.contains_key("key1"));
        assert!(context.contains_key("key2"));
        assert!(!context.contains_key("key3"));

        let keys: Vec<&String> = context.keys().collect();
        assert_eq!(keys.len(), 2);
        assert!(keys.contains(&&"key1".to_string()));
        assert!(keys.contains(&&"key2".to_string()));
    }

    #[test]
    fn test_request_context_remove() {
        let mut context = RequestContext::new();
        
        context.set("removable", "test_value".to_string());
        assert!(context.contains_key("removable"));

        let removed: Option<String> = context.remove("removable");
        assert_eq!(removed, Some("test_value".to_string()));
        assert!(!context.contains_key("removable"));

        // 既に削除済みのキー
        let removed: Option<String> = context.remove("removable");
        assert_eq!(removed, None);
    }

    #[test]
    fn test_request_context_clear() {
        let mut context = RequestContext::new();
        
        context.set("key1", "value1".to_string());
        context.set("key2", 42);
        assert!(!context.is_empty());

        context.clear();
        assert!(context.is_empty());
        assert!(!context.contains_key("key1"));
        assert!(!context.contains_key("key2"));
    }

    #[test]
    fn test_request_with_context() {
        let mut context = RequestContext::new();
        context.set("user_id", 123u32);
        context.set("authenticated", true);

        let mut req = Request::new(Method::GET, "/test".to_string())
            .with_context(context);

        // コンテキストから値を取得
        assert_eq!(req.context().get::<u32>("user_id"), Some(&123u32));
        assert_eq!(req.context().get::<bool>("authenticated"), Some(&true));

        // 可変参照で値を追加
        req.context_mut().set("role", "admin".to_string());
        assert_eq!(req.context().get::<String>("role"), Some(&"admin".to_string()));
    }

    #[derive(Debug, Clone, PartialEq)]
    struct UserInfo {
        id: u32,
        name: String,
    }

    #[test]
    fn test_request_context_custom_types() {
        let mut context = RequestContext::new();
        
        let user = UserInfo { id: 42, name: "Alice".to_string() };
        context.set("user", user.clone());

        let retrieved_user = context.get::<UserInfo>("user");
        assert_eq!(retrieved_user, Some(&user));

        let removed_user: Option<UserInfo> = context.remove("user");
        assert_eq!(removed_user, Some(user));
    }

    #[test]
    fn test_cookie_basic() {
        let cookie = Cookie::new("session_id", "abc123");
        
        assert_eq!(cookie.name, "session_id");
        assert_eq!(cookie.value, "abc123");
        assert_eq!(cookie.path, None);
        assert_eq!(cookie.domain, None);
        assert!(!cookie.secure);
        assert!(!cookie.http_only);
        assert_eq!(cookie.same_site, None);
    }

    #[test]
    fn test_cookie_builder() {
        let cookie = Cookie::new("auth_token", "xyz789")
            .with_path("/")
            .with_domain("example.com")
            .secure(true)
            .http_only(true)
            .with_same_site(SameSite::Strict);

        assert_eq!(cookie.path, Some("/".to_string()));
        assert_eq!(cookie.domain, Some("example.com".to_string()));
        assert!(cookie.secure);
        assert!(cookie.http_only);
        assert_eq!(cookie.same_site, Some(SameSite::Strict));
    }

    #[test]
    fn test_cookie_header_value() {
        let cookie = Cookie::new("test", "value")
            .with_path("/app")
            .with_domain("test.com")
            .secure(true)
            .http_only(true)
            .with_same_site(SameSite::Lax);

        let header_value = cookie.to_header_value();
        
        assert!(header_value.contains("test=value"));
        assert!(header_value.contains("Path=/app"));
        assert!(header_value.contains("Domain=test.com"));
        assert!(header_value.contains("Secure"));
        assert!(header_value.contains("HttpOnly"));
        assert!(header_value.contains("SameSite=Lax"));
    }

    #[test]
    fn test_cookie_with_expires() {
        use chrono::{TimeZone, Utc};
        
        let expires = Utc.with_ymd_and_hms(2024, 12, 31, 23, 59, 59).unwrap();
        let cookie = Cookie::new("expires_test", "value")
            .with_expires(expires);

        let header_value = cookie.to_header_value();
        assert!(header_value.contains("Expires=Tue, 31 Dec 2024 23:59:59 GMT"));
    }

    #[test]
    fn test_cookie_with_max_age() {
        let max_age = Duration::from_secs(3600); // 1 hour
        let cookie = Cookie::new("max_age_test", "value")
            .with_max_age(max_age);

        let header_value = cookie.to_header_value();
        assert!(header_value.contains("Max-Age=3600"));
    }

    #[test]
    fn test_same_site_display() {
        assert_eq!(SameSite::Strict.to_string(), "Strict");
        assert_eq!(SameSite::Lax.to_string(), "Lax");
        assert_eq!(SameSite::None.to_string(), "None");
    }

    #[cfg(feature = "cgi")]
    #[test]
    fn test_cgi_utils_mock() {
        // CGI環境をモックしてテスト
        // 実際のテストでは環境変数を設定する必要がある
        use cgi_utils::*;
        use std::env;

        // 環境変数を設定
        env::set_var("HTTP_COOKIE", "session=abc123; user_id=456");
        env::set_var("HTTP_USER_AGENT", "TestAgent/1.0");
        env::set_var("CONTENT_TYPE", "application/json");

        let cookies = extract_cookies();
        assert_eq!(cookies.get("session"), Some(&"abc123".to_string()));
        assert_eq!(cookies.get("user_id"), Some(&"456".to_string()));

        let headers = extract_headers();
        assert_eq!(headers.get("user-agent"), Some(&"TestAgent/1.0".to_string()));
        assert_eq!(headers.get("content-type"), Some(&"application/json".to_string()));

        // クリーンアップ
        env::remove_var("HTTP_COOKIE");
        env::remove_var("HTTP_USER_AGENT");
        env::remove_var("CONTENT_TYPE");
    }

    #[cfg(feature = "cgi")]
    #[test]
    fn test_set_cookie_response() {
        use cgi_utils::*;
        
        let mut response = Response::new(200);
        let cookie = Cookie::new("test_cookie", "test_value")
            .with_path("/")
            .secure(true);

        set_cookie(&mut response, cookie);

        let set_cookie_header = response.headers.get("Set-Cookie");
        assert!(set_cookie_header.is_some());
        
        let header_value = set_cookie_header.unwrap();
        assert!(header_value.contains("test_cookie=test_value"));
        assert!(header_value.contains("Path=/"));
        assert!(header_value.contains("Secure"));
    }
} 