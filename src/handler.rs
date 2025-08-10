//! ハンドラーの実装

use std::marker::PhantomData;
use std::future::Future;
use std::sync::OnceLock;
use async_trait::async_trait;
use serde::{Serialize, de::DeserializeOwned};
use log::{debug, info, warn, error};
use regex::Regex;
use futures::future::{self, Ready};

#[cfg(debug_assertions)]
use std::time::{Duration, Instant};

use crate::common::{Handler, Method, Request, Response};
use crate::error::Error;

/// Content-Typeの許容範囲を判定（拡張しやすい実装）
fn is_json_like_content_type(ct: &str) -> bool {
    let main_type = ct.split(';').next().unwrap_or("").trim().to_ascii_lowercase();
    // 明示リスト（将来拡張しやすい）
    const EXTRA_ALLOWED: &[&str] = &[
        // RFC 7464 JSON Text Sequences（ボディ仕様は異なるが、将来的拡張を想定）
        "application/json-seq",
    ];

    main_type == "application/json"
        || main_type.ends_with("+json")
        || EXTRA_ALLOWED.contains(&main_type.as_str())
}

/// パターンの安全性を確保（アンカーの確認と追加）
fn ensure_safe_pattern(pattern: &str) -> Result<String, Error> {
    // 空のパターンや特殊ケースのハンドリング - エラーとして弾く
    if pattern.is_empty() {
        return Err(Error::InvalidRequestBody("Empty regex pattern is not allowed".to_string()));
    }
    
    // アンカーの確認と追加
    let has_start_anchor = pattern.starts_with('^');
    let has_end_anchor = pattern.ends_with('$');
    
    if !has_start_anchor || !has_end_anchor {
        let safe_pattern = format!("^{}$", 
            pattern.trim_start_matches('^').trim_end_matches('$'));
        warn!("Pattern '{}' lacks proper anchors, converted to '{}' for security", 
              pattern, safe_pattern);
        Ok(safe_pattern)
    } else {
        Ok(pattern.to_string())
    }
}

/// レスポンス変換トレイト
pub trait ResponseWrapper {
    /// 自身をResponseに変換
    fn into_response(self) -> Result<Response, Error>;
}

/// 通常のシリアライズ可能なデータ型に対するResponseWrapper実装
impl<T: Serialize> ResponseWrapper for T {
    fn into_response(self) -> Result<Response, Error> {
        Response::ok().json(&self)
    }
}

/// Response型に対するResponseWrapper実装（恒等関数）
impl ResponseWrapper for Response {
    fn into_response(self) -> Result<Response, Error> {
        Ok(self)
    }
}

/// ルートハンドラー
pub struct RouteHandler<F, T, R>
where
    F: Fn(Request, Option<T>) -> Result<R, Error> + Send + Sync + 'static,
    T: DeserializeOwned + Send + Sync + 'static,
    R: ResponseWrapper + Send + Sync + 'static,
{
    /// ルートパス（正規表現パターン）
    pub path_pattern: String,
    /// コンパイル済み正規表現（キャッシュ）
    pub compiled_regex: OnceLock<Result<Regex, regex::Error>>,
    /// HTTPメソッド
    pub method: Method,
    /// ハンドラー関数
    pub handler_fn: F,
    /// リクエストボディの型
    pub _request_type: PhantomData<T>,
    /// レスポンスボディの型
    pub _response_type: PhantomData<R>,
}

impl<F, T, R> RouteHandler<F, T, R>
where
    F: Fn(Request, Option<T>) -> Result<R, Error> + Send + Sync + 'static,
    T: DeserializeOwned + Send + Sync + 'static,
    R: ResponseWrapper + Send + Sync + 'static,
{
    /// 新しいRouteHandlerを作成
    pub fn try_new(method: Method, path_pattern: impl Into<String>, handler_fn: F) -> Result<Self, Error> {
        let pattern = path_pattern.into();
        
        // パターンの安全性チェック
        let safe_pattern = ensure_safe_pattern(&pattern)?;
        
        // 開発時はinfo、本番相当ではdebugに落とす
        #[cfg(debug_assertions)]
        info!("Registering handler for {} with pattern: {}", method, safe_pattern);
        #[cfg(not(debug_assertions))]
        debug!("Registering handler for {} with pattern: {}", method, safe_pattern);
        Ok(Self {
            method,
            path_pattern: safe_pattern,
            compiled_regex: OnceLock::new(),
            handler_fn,
            _request_type: PhantomData,
            _response_type: PhantomData,
        })
    }
    
    /// 新しいRouteHandlerを作成（従来のAPI、非推奨）
    #[deprecated(note = "Use try_new instead for better error handling")]
    pub fn new(method: Method, path_pattern: impl Into<String>, handler_fn: F) -> Self {
        Self::try_new(method, path_pattern, handler_fn)
            .unwrap_or_else(|e| {
                panic!("Failed to create RouteHandler: {}", e);
            })
    }
}

/// 非同期ルートハンドラー
pub struct AsyncRouteHandler<F, T, R, Fut>
where
    F: Fn(Request, Option<T>) -> Fut + Send + Sync + 'static,
    T: DeserializeOwned + Send + Sync + 'static,
    R: ResponseWrapper + Send + Sync + 'static,
    Fut: Future<Output = Result<R, Error>> + Send + Sync + 'static,
{
    /// ルートパス（正規表現パターン）
    pub path_pattern: String,
    /// コンパイル済み正規表現（キャッシュ）
    pub compiled_regex: OnceLock<Result<Regex, regex::Error>>,
    /// HTTPメソッド
    pub method: Method,
    /// 非同期ハンドラー関数
    pub handler_fn: F,
    /// リクエストボディの型
    pub _request_type: PhantomData<T>,
    /// レスポンスボディの型
    pub _response_type: PhantomData<R>,
    /// Future型
    pub _future_type: PhantomData<Fut>,
}

impl<F, T, R, Fut> AsyncRouteHandler<F, T, R, Fut>
where
    F: Fn(Request, Option<T>) -> Fut + Send + Sync + 'static,
    T: DeserializeOwned + Send + Sync + 'static,
    R: ResponseWrapper + Send + Sync + 'static,
    Fut: Future<Output = Result<R, Error>> + Send + Sync + 'static,
{
    /// 新しいAsyncRouteHandlerを作成
    pub fn try_new(method: Method, path_pattern: impl Into<String>, handler_fn: F) -> Result<Self, Error> {
        let pattern = path_pattern.into();
        
        // パターンの安全性チェック
        let safe_pattern = ensure_safe_pattern(&pattern)?;
        
        // 開発時はinfo、本番相当ではdebugに落とす
        #[cfg(debug_assertions)]
        info!("Registering async handler for {} with pattern: {}", method, safe_pattern);
        #[cfg(not(debug_assertions))]
        debug!("Registering async handler for {} with pattern: {}", method, safe_pattern);
        Ok(Self {
            method,
            path_pattern: safe_pattern,
            compiled_regex: OnceLock::new(),
            handler_fn,
            _request_type: PhantomData,
            _response_type: PhantomData,
            _future_type: PhantomData,
        })
    }
    
    /// 新しいAsyncRouteHandlerを作成（従来のAPI、非推奨）
    #[deprecated(note = "Use try_new instead for better error handling")]
    pub fn new(method: Method, path_pattern: impl Into<String>, handler_fn: F) -> Self {
        Self::try_new(method, path_pattern, handler_fn)
            .unwrap_or_else(|e| {
                panic!("Failed to create AsyncRouteHandler: {}", e);
            })
    }
}

#[async_trait]
impl<F, T, R> Handler for RouteHandler<F, T, R>
where
    F: Fn(Request, Option<T>) -> Result<R, Error> + Send + Sync + 'static,
    T: DeserializeOwned + Send + Sync + 'static,
    R: ResponseWrapper + Send + Sync + 'static,
{
    fn matches(&self, path: &str, method: &Method) -> bool {
        if method != &self.method {
            return false;
        }

        // コンパイル済み正規表現を取得またはコンパイル
        let compiled_result = self.compiled_regex.get_or_init(|| {
            Regex::new(&self.path_pattern)
        });

        match compiled_result {
            Ok(regex) => {
                // デバッグビルド時のみタイムアウト監視
                #[cfg(debug_assertions)]
                {
                    let start_time = Instant::now();
                    let is_match = regex.is_match(path);
                    let elapsed = start_time.elapsed();
                    
                    // 100msを超えた場合は警告ログを出力
                    if elapsed > Duration::from_millis(100) {
                        warn!("Slow regex matching detected: pattern '{}' took {:?} for path '{}'", 
                              self.path_pattern, elapsed, path);
                    }
                    
                    debug!("Path matching: {} against pattern {}: {} (took {:?})", 
                           path, self.path_pattern, is_match, elapsed);
                    is_match
                }
                #[cfg(not(debug_assertions))]
                {
                    let is_match = regex.is_match(path);
                    debug!("Path matching: {} against pattern {}: {}", 
                           path, self.path_pattern, is_match);
                    is_match
                }
            },
            Err(e) => {
                error!("Invalid regex pattern: {} - {}. Pattern will be rejected for security.", 
                       self.path_pattern, e);
                // 無効な正規表現の場合はマッチしない（設定ミスを隠蔽しない）
                false
            }
        }
    }

    // 追加: パスパターンを返すメソッドの実装
    fn path_pattern(&self) -> &str {
        &self.path_pattern
    }

    async fn handle(&self, req: Request) -> Result<Response, Error> {
        // リクエストボディが長さ>0のときのみContent-Type検証とJSONパースを行う
        let has_non_empty_body = req.body.as_ref().map(|b| !b.is_empty()).unwrap_or(false);
        let body_data = if has_non_empty_body {
            // 取込み時にヘッダーは小文字化されている前提
            let content_type = req.headers.get("content-type").cloned();

            let ct = content_type.ok_or_else(|| {
                warn!("Request with body missing Content-Type header");
                Error::InvalidRequestBody("Missing Content-Type header".to_string())
            })?;

            // 許容範囲の判定（application/json, *+json, リスト拡張）
            if !is_json_like_content_type(&ct) {
                warn!("Unsupported Content-Type for JSON parsing: {}", ct);
                return Err(Error::InvalidRequestBody(format!(
                    "Unsupported Content-Type: {} (expected application/json or *+json)",
                    ct
                )));
            }

            Some(req.json::<T>()?)
        } else {
            None
        };

        // ハンドラー関数を実行
        let result = (self.handler_fn)(req, body_data)?;

        // 結果をResponseに変換
        result.into_response()
    }
}

#[async_trait]
impl<F, T, R, Fut> Handler for AsyncRouteHandler<F, T, R, Fut>
where
    F: Fn(Request, Option<T>) -> Fut + Send + Sync + 'static,
    T: DeserializeOwned + Send + Sync + 'static,
    R: ResponseWrapper + Send + Sync + 'static,
    Fut: Future<Output = Result<R, Error>> + Send + Sync + 'static,
{
    fn matches(&self, path: &str, method: &Method) -> bool {
        if method != &self.method {
            return false;
        }

        // コンパイル済み正規表現を取得またはコンパイル
        let compiled_result = self.compiled_regex.get_or_init(|| {
            Regex::new(&self.path_pattern)
        });

        match compiled_result {
            Ok(regex) => {
                // デバッグビルド時のみタイムアウト監視
                #[cfg(debug_assertions)]
                {
                    let start_time = Instant::now();
                    let is_match = regex.is_match(path);
                    let elapsed = start_time.elapsed();
                    
                    // 100msを超えた場合は警告ログを出力
                    if elapsed > Duration::from_millis(100) {
                        warn!("Slow regex matching detected: pattern '{}' took {:?} for path '{}'", 
                              self.path_pattern, elapsed, path);
                    }
                    
                    debug!("Path matching: {} against pattern {}: {} (took {:?})", 
                           path, self.path_pattern, is_match, elapsed);
                    is_match
                }
                #[cfg(not(debug_assertions))]
                {
                    let is_match = regex.is_match(path);
                    debug!("Path matching: {} against pattern {}: {}", 
                           path, self.path_pattern, is_match);
                    is_match
                }
            },
            Err(e) => {
                error!("Invalid regex pattern: {} - {}. Pattern will be rejected for security.", 
                       self.path_pattern, e);
                // 無効な正規表現の場合はマッチしない（設定ミスを隠蔽しない）
                false
            }
        }
    }

    // 追加: パスパターンを返すメソッドの実装
    fn path_pattern(&self) -> &str {
        &self.path_pattern
    }

    async fn handle(&self, req: Request) -> Result<Response, Error> {
        // リクエストボディが長さ>0のときのみContent-Type検証とJSONパースを行う
        let has_non_empty_body = req.body.as_ref().map(|b| !b.is_empty()).unwrap_or(false);
        let body_data = if has_non_empty_body {
            // 取込み時にヘッダーは小文字化されている前提
            let content_type = req.headers.get("content-type").cloned();

            let ct = content_type.ok_or_else(|| {
                warn!("Request with body missing Content-Type header");
                Error::InvalidRequestBody("Missing Content-Type header".to_string())
            })?;

            // 許容範囲の判定（application/json, *+json, リスト拡張）
            if !is_json_like_content_type(&ct) {
                warn!("Unsupported Content-Type for JSON parsing: {}", ct);
                return Err(Error::InvalidRequestBody(format!(
                    "Unsupported Content-Type: {} (expected application/json or *+json)",
                    ct
                )));
            }

            Some(req.json::<T>()?)
        } else {
            None
        };

        // 非同期ハンドラー関数を実行
        let result = (self.handler_fn)(req, body_data).await?;

        // 結果をResponseに変換
        result.into_response()
    }
}

/// マクロでHTTPハンドラーを生成するための補助関数
pub fn get<F, R>(path: impl Into<String>, handler: F) -> RouteHandler<impl Fn(Request, Option<()>) -> Result<R, Error> + Send + Sync + 'static, (), R>
where
    F: Fn(Request) -> Result<R, Error> + Send + Sync + 'static,
    R: ResponseWrapper + Send + Sync + 'static,
{
    #[allow(deprecated)]
    RouteHandler::new(Method::GET, path, move |req, _| handler(req))
}

/// マクロでHTTPハンドラーを生成するための補助関数（エラーハンドリング付き）
pub fn try_get<F, R>(path: impl Into<String>, handler: F) -> Result<RouteHandler<impl Fn(Request, Option<()>) -> Result<R, Error> + Send + Sync + 'static, (), R>, Error>
where
    F: Fn(Request) -> Result<R, Error> + Send + Sync + 'static,
    R: ResponseWrapper + Send + Sync + 'static,
{
    RouteHandler::try_new(Method::GET, path, move |req, _| handler(req))
}

/// 非同期GETハンドラーを作成
pub fn async_get<F, R, Fut>(path: impl Into<String>, handler: F) -> AsyncRouteHandler<impl Fn(Request, Option<()>) -> Fut + Send + Sync + 'static, (), R, Fut>
where
    F: Fn(Request) -> Fut + Send + Sync + 'static,
    R: ResponseWrapper + Send + Sync + 'static,
    Fut: Future<Output = Result<R, Error>> + Send + Sync + 'static,
{
    #[allow(deprecated)]
    AsyncRouteHandler::new(Method::GET, path, move |req, _| handler(req))
}

/// 非同期GETハンドラーを作成（エラーハンドリング付き）
pub fn try_async_get<F, R, Fut>(path: impl Into<String>, handler: F) -> Result<AsyncRouteHandler<impl Fn(Request, Option<()>) -> Fut + Send + Sync + 'static, (), R, Fut>, Error>
where
    F: Fn(Request) -> Fut + Send + Sync + 'static,
    R: ResponseWrapper + Send + Sync + 'static,
    Fut: Future<Output = Result<R, Error>> + Send + Sync + 'static,
{
    AsyncRouteHandler::try_new(Method::GET, path, move |req, _| handler(req))
}

/// POSTハンドラーを作成
pub fn post<F, T, R>(path: impl Into<String>, handler: F) -> RouteHandler<impl Fn(Request, Option<T>) -> Result<R, Error> + Send + Sync + 'static, T, R>
where
    F: Fn(Request, T) -> Result<R, Error> + Send + Sync + 'static,
    T: DeserializeOwned + Send + Sync + 'static,
    R: ResponseWrapper + Send + Sync + 'static,
{
    #[allow(deprecated)]
    RouteHandler::new(Method::POST, path, move |req, body_data| {
        if let Some(data) = body_data {
            handler(req, data)
        } else {
            Err(Error::InvalidRequestBody("Missing request body".to_string()))
        }
    })
}

/// 非同期POSTハンドラーを作成
pub fn async_post<F, T, R, Fut>(path: impl Into<String>, handler: F) -> AsyncRouteHandler<impl Fn(Request, Option<T>) -> future::Either<Ready<Result<R, Error>>, Fut> + Send + Sync + 'static, T, R, future::Either<Ready<Result<R, Error>>, Fut>>
where
    F: Fn(Request, T) -> Fut + Send + Sync + 'static,
    T: DeserializeOwned + Send + Sync + 'static,
    R: ResponseWrapper + Send + Sync + 'static,
    Fut: Future<Output = Result<R, Error>> + Send + Sync + 'static,
{
    #[allow(deprecated)]
    AsyncRouteHandler::new(Method::POST, path, move |req, body_data| {
        if let Some(data) = body_data {
            future::Either::Right(handler(req, data))
        } else {
            future::Either::Left(future::ready(Err(Error::InvalidRequestBody("Missing request body".to_string()))))
        }
    })
}

/// PUTハンドラーを作成
pub fn put<F, T, R>(path: impl Into<String>, handler: F) -> RouteHandler<impl Fn(Request, Option<T>) -> Result<R, Error> + Send + Sync + 'static, T, R>
where
    F: Fn(Request, T) -> Result<R, Error> + Send + Sync + 'static,
    T: DeserializeOwned + Send + Sync + 'static,
    R: ResponseWrapper + Send + Sync + 'static,
{
    #[allow(deprecated)]
    RouteHandler::new(Method::PUT, path, move |req, body_data| {
        if let Some(data) = body_data {
            handler(req, data)
        } else {
            Err(Error::InvalidRequestBody("Missing request body".to_string()))
        }
    })
}

/// 非同期PUTハンドラーを作成
pub fn async_put<F, T, R, Fut>(path: impl Into<String>, handler: F) -> AsyncRouteHandler<impl Fn(Request, Option<T>) -> future::Either<Ready<Result<R, Error>>, Fut> + Send + Sync + 'static, T, R, future::Either<Ready<Result<R, Error>>, Fut>>
where
    F: Fn(Request, T) -> Fut + Send + Sync + 'static,
    T: DeserializeOwned + Send + Sync + 'static,
    R: ResponseWrapper + Send + Sync + 'static,
    Fut: Future<Output = Result<R, Error>> + Send + Sync + 'static,
{
    #[allow(deprecated)]
    AsyncRouteHandler::new(Method::PUT, path, move |req, body_data| {
        if let Some(data) = body_data {
            future::Either::Right(handler(req, data))
        } else {
            future::Either::Left(future::ready(Err(Error::InvalidRequestBody("Missing request body".to_string()))))
        }
    })
}

/// DELETEハンドラーを作成
pub fn delete<F, R>(path: impl Into<String>, handler: F) -> RouteHandler<impl Fn(Request, Option<()>) -> Result<R, Error> + Send + Sync + 'static, (), R>
where
    F: Fn(Request) -> Result<R, Error> + Send + Sync + 'static,
    R: ResponseWrapper + Send + Sync + 'static,
{
    #[allow(deprecated)]
    RouteHandler::new(Method::DELETE, path, move |req, _| handler(req))
}

/// 非同期DELETEハンドラーを作成
pub fn async_delete<F, R, Fut>(path: impl Into<String>, handler: F) -> AsyncRouteHandler<impl Fn(Request, Option<()>) -> Fut + Send + Sync + 'static, (), R, Fut>
where
    F: Fn(Request) -> Fut + Send + Sync + 'static,
    R: ResponseWrapper + Send + Sync + 'static,
    Fut: Future<Output = Result<R, Error>> + Send + Sync + 'static,
{
    #[allow(deprecated)]
    AsyncRouteHandler::new(Method::DELETE, path, move |req, _| handler(req))
}

/// OPTIONSハンドラーを作成
pub fn options<F, R>(path: impl Into<String>, handler: F) -> RouteHandler<impl Fn(Request, Option<()>) -> Result<R, Error> + Send + Sync + 'static, (), R>
where
    F: Fn(Request) -> Result<R, Error> + Send + Sync + 'static,
    R: ResponseWrapper + Send + Sync + 'static,
{
    #[allow(deprecated)]
    RouteHandler::new(Method::OPTIONS, path, move |req, _| handler(req))
}

/// 非同期OPTIONSハンドラーを作成
pub fn async_options<F, R, Fut>(path: impl Into<String>, handler: F) -> AsyncRouteHandler<impl Fn(Request, Option<()>) -> Fut + Send + Sync + 'static, (), R, Fut>
where
    F: Fn(Request) -> Fut + Send + Sync + 'static,
    R: ResponseWrapper + Send + Sync + 'static,
    Fut: Future<Output = Result<R, Error>> + Send + Sync + 'static,
{
    #[allow(deprecated)]
    AsyncRouteHandler::new(Method::OPTIONS, path, move |req, _| handler(req))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Serialize, Deserialize};

    #[derive(Serialize, Deserialize, Debug, PartialEq)]
    struct TestRequest {
        name: String,
        value: i32,
    }

    #[derive(Serialize, Deserialize, Debug, PartialEq)]
    struct TestResponse {
        message: String,
        value: i32,
    }

    fn test_get_handler(_req: Request) -> Result<TestResponse, Error> {
        Ok(TestResponse {
            message: "Hello from GET".to_string(),
            value: 42,
        })
    }

    fn test_post_handler(_req: Request, body: TestRequest) -> Result<TestResponse, Error> {
        Ok(TestResponse {
            message: format!("Hello, {}", body.name),
            value: body.value * 2,
        })
    }

    // 非同期ハンドラー関数
    async fn test_async_get_handler(_req: Request) -> Result<TestResponse, Error> {
        Ok(TestResponse {
            message: "Hello from async GET".to_string(),
            value: 100,
        })
    }

    async fn test_async_post_handler(_req: Request, body: TestRequest) -> Result<TestResponse, Error> {
        Ok(TestResponse {
            message: format!("Hello async, {}", body.name),
            value: body.value * 3,
        })
    }

    fn test_options_handler(_req: Request) -> Result<TestResponse, Error> {
        Ok(TestResponse {
            message: "Hello from OPTIONS".to_string(),
            value: 200,
        })
    }
    
    async fn test_async_options_handler(_req: Request) -> Result<TestResponse, Error> {
        Ok(TestResponse {
            message: "Hello from async OPTIONS".to_string(),
            value: 204,
        })
    }

    // カスタムヘッダーを返すハンドラー
    fn test_custom_header_handler(_req: Request) -> Result<Response, Error> {
        let response_data = TestResponse {
            message: "Response with custom header".to_string(),
            value: 123,
        };
        
        Ok(Response::ok()
            .with_header("X-Custom-Header", "CustomValue")
            .with_header("X-API-Version", "1.0")
            .json(&response_data)?)
    }
    
    // 非同期でカスタムヘッダーを返すハンドラー
    async fn test_async_custom_header_handler(_req: Request) -> Result<Response, Error> {
        let response_data = TestResponse {
            message: "Async response with custom header".to_string(),
            value: 456,
        };
        
        Ok(Response::ok()
            .with_header("X-Custom-Header", "AsyncValue")
            .with_header("X-API-Version", "2.0")
            .json(&response_data)?)
    }

    #[tokio::test]
    async fn test_get_handler_matches() {
        let handler = get("/test", test_get_handler);
        
        assert!(handler.matches("/test", &Method::GET));
        assert!(!handler.matches("/test", &Method::POST));
        assert!(!handler.matches("/other", &Method::GET));
    }

    #[tokio::test]
    async fn test_post_handler_matches() {
        let handler = post("/users", test_post_handler);
        
        assert!(handler.matches("/users", &Method::POST));
        assert!(!handler.matches("/users", &Method::GET));
        assert!(!handler.matches("/items", &Method::POST));
    }

    #[tokio::test]
    async fn test_get_handler_execution() {
        let handler = get("/test", test_get_handler);
        let req = Request::new(Method::GET, "/test".to_string());
        
        let result = handler.handle(req).await.unwrap();
        
        assert_eq!(result.status, 200);
        
        // レスポンスボディを検証
        let body_str = String::from_utf8(result.body.unwrap()).unwrap();
        let response: TestResponse = serde_json::from_str(&body_str).unwrap();
        
        assert_eq!(response.message, "Hello from GET");
        assert_eq!(response.value, 42);
    }

    #[tokio::test]
    async fn test_post_handler_execution() {
        let handler = post("/users", test_post_handler);
        
        let test_data = TestRequest {
            name: "Test User".to_string(),
            value: 21,
        };
        
        let json_body = serde_json::to_vec(&test_data).unwrap();
        let req = Request::new(Method::POST, "/users".to_string())
            .with_header("Content-Type", "application/json; charset=utf-8")
            .with_body(json_body);
        
        let result = handler.handle(req).await.unwrap();
        
        assert_eq!(result.status, 200);
        
        // レスポンスボディを検証
        let body_str = String::from_utf8(result.body.unwrap()).unwrap();
        let response: TestResponse = serde_json::from_str(&body_str).unwrap();
        
        assert_eq!(response.message, "Hello, Test User");
        assert_eq!(response.value, 42); // 21 * 2
    }

    #[tokio::test]
    async fn test_post_handler_missing_body() {
        let handler = post("/users", test_post_handler);
        let req = Request::new(Method::POST, "/users".to_string());
        
        let result = handler.handle(req).await;
        
        assert!(result.is_err());
        match result {
            Err(Error::InvalidRequestBody(_)) => {},
            _ => panic!("Expected InvalidRequestBody error"),
        }
    }

    #[tokio::test]
    async fn test_regex_path_pattern() {
        // 正規表現パターンによるパスマッチングのテスト
        let handler = get(r"^/items/\d+$", test_get_handler);
        
        assert!(handler.matches("/items/123", &Method::GET));
        assert!(handler.matches("/items/456", &Method::GET));
        assert!(!handler.matches("/items/abc", &Method::GET));
        assert!(!handler.matches("/items/", &Method::GET));
    }

    // 非同期ハンドラーのテスト
    #[tokio::test]
    async fn test_async_get_handler_matches() {
        let handler = async_get("/test", test_async_get_handler);
        
        assert!(handler.matches("/test", &Method::GET));
        assert!(!handler.matches("/test", &Method::POST));
        assert!(!handler.matches("/other", &Method::GET));
    }

    #[tokio::test]
    async fn test_async_post_handler_matches() {
        let handler = async_post("/users", test_async_post_handler);
        
        assert!(handler.matches("/users", &Method::POST));
        assert!(!handler.matches("/users", &Method::GET));
        assert!(!handler.matches("/items", &Method::POST));
    }

    #[tokio::test]
    async fn test_async_get_handler_execution() {
        let handler = async_get("/test", test_async_get_handler);
        let req = Request::new(Method::GET, "/test".to_string());
        
        let result = handler.handle(req).await.unwrap();
        
        assert_eq!(result.status, 200);
        
        // レスポンスボディを検証
        let body_str = String::from_utf8(result.body.unwrap()).unwrap();
        let response: TestResponse = serde_json::from_str(&body_str).unwrap();
        
        assert_eq!(response.message, "Hello from async GET");
        assert_eq!(response.value, 100);
    }

    #[tokio::test]
    async fn test_async_post_handler_execution() {
        let handler = async_post("/users", test_async_post_handler);
        
        let test_data = TestRequest {
            name: "Test User".to_string(),
            value: 21,
        };
        
        let json_body = serde_json::to_vec(&test_data).unwrap();
        let req = Request::new(Method::POST, "/users".to_string())
            .with_header("Content-Type", "application/json")
            .with_body(json_body);
        
        let result = handler.handle(req).await.unwrap();
        
        assert_eq!(result.status, 200);
        
        // レスポンスボディを検証
        let body_str = String::from_utf8(result.body.unwrap()).unwrap();
        let response: TestResponse = serde_json::from_str(&body_str).unwrap();
        
        assert_eq!(response.message, "Hello async, Test User");
        assert_eq!(response.value, 63); // 21 * 3
    }

    #[tokio::test]
    async fn test_async_post_handler_missing_body() {
        let handler = async_post("/users", test_async_post_handler);
        let req = Request::new(Method::POST, "/users".to_string());
        
        let result = handler.handle(req).await;
        
        assert!(result.is_err());
        match result {
            Err(Error::InvalidRequestBody(_)) => {},
            _ => panic!("Expected InvalidRequestBody error"),
        }
    }

    #[tokio::test]
    async fn test_options_handler_matches() {
        let handler = options("/cors-test", test_options_handler);
        
        assert!(handler.matches("/cors-test", &Method::OPTIONS));
        assert!(!handler.matches("/cors-test", &Method::GET));
        assert!(!handler.matches("/other", &Method::OPTIONS));
    }
    
    #[tokio::test]
    async fn test_options_handler_execution() {
        let handler = options("/cors-test", test_options_handler);
        let req = Request::new(Method::OPTIONS, "/cors-test".to_string());
        
        let result = handler.handle(req).await.unwrap();
        
        assert_eq!(result.status, 200);
        
        // レスポンスボディを検証
        let body_str = String::from_utf8(result.body.unwrap()).unwrap();
        let response: TestResponse = serde_json::from_str(&body_str).unwrap();
        
        assert_eq!(response.message, "Hello from OPTIONS");
        assert_eq!(response.value, 200);
    }
    
    #[tokio::test]
    async fn test_async_options_handler_matches() {
        let handler = async_options("/cors-test", test_async_options_handler);
        
        assert!(handler.matches("/cors-test", &Method::OPTIONS));
        assert!(!handler.matches("/cors-test", &Method::GET));
        assert!(!handler.matches("/other", &Method::OPTIONS));
    }
    
    #[tokio::test]
    async fn test_async_options_handler_execution() {
        let handler = async_options("/cors-test", test_async_options_handler);
        let req = Request::new(Method::OPTIONS, "/cors-test".to_string());
        
        let result = handler.handle(req).await.unwrap();
        
        assert_eq!(result.status, 200);
        
        // レスポンスボディを検証
        let body_str = String::from_utf8(result.body.unwrap()).unwrap();
        let response: TestResponse = serde_json::from_str(&body_str).unwrap();
        
        assert_eq!(response.message, "Hello from async OPTIONS");
        assert_eq!(response.value, 204);
    }

    #[tokio::test]
    async fn test_custom_header_handler_execution() {
        let handler = get("/custom-header", test_custom_header_handler);
        let req = Request::new(Method::GET, "/custom-header".to_string());
        
        let result = handler.handle(req).await.unwrap();
        
        assert_eq!(result.status, 200);
        assert_eq!(result.headers.get("X-Custom-Header"), Some(&"CustomValue".to_string()));
        assert_eq!(result.headers.get("X-API-Version"), Some(&"1.0".to_string()));
        
        // レスポンスボディを検証
        let body_str = String::from_utf8(result.body.unwrap()).unwrap();
        let response: TestResponse = serde_json::from_str(&body_str).unwrap();
        
        assert_eq!(response.message, "Response with custom header");
        assert_eq!(response.value, 123);
    }
    
    #[tokio::test]
    async fn test_async_custom_header_handler_execution() {
        let handler = async_get("/async-custom-header", test_async_custom_header_handler);
        let req = Request::new(Method::GET, "/async-custom-header".to_string());
        
        let result = handler.handle(req).await.unwrap();
        
        assert_eq!(result.status, 200);
        assert_eq!(result.headers.get("X-Custom-Header"), Some(&"AsyncValue".to_string()));
        assert_eq!(result.headers.get("X-API-Version"), Some(&"2.0".to_string()));
        
        // レスポンスボディを検証
        let body_str = String::from_utf8(result.body.unwrap()).unwrap();
        let response: TestResponse = serde_json::from_str(&body_str).unwrap();
        
        assert_eq!(response.message, "Async response with custom header");
        assert_eq!(response.value, 456);
    }
    
    #[tokio::test]
    async fn test_invalid_regex_pattern_fail_closed() {
        // 無効な正規表現パターンでハンドラーを作成
        let handler = get(r"^[", test_get_handler); // 無効な正規表現
        
        // どんなパスでもマッチしないことを確認（fail-closed）
        assert!(!handler.matches("/test", &Method::GET));
        assert!(!handler.matches("[", &Method::GET));
        assert!(!handler.matches("/anything", &Method::GET));
        assert!(!handler.matches("", &Method::GET));
    }
    
    #[tokio::test]
    async fn test_empty_pattern_rejection() {
        // 空のパターンでtry_newを使った場合のエラーハンドリングをテスト
        let result = RouteHandler::try_new(Method::GET, "", move |req, _: Option<()>| test_get_handler(req));
        assert!(result.is_err());
        
        let result = AsyncRouteHandler::try_new(Method::GET, "", move |req, _: Option<()>| test_async_get_handler(req));
        assert!(result.is_err());
    }
    
    #[tokio::test]
    async fn test_anchor_normalization() {
        // アンカーなしパターンの正規化テスト
        let handler = get("/test", test_get_handler); // アンカーなし
        
        // 正確なマッチのみ成功することを確認
        assert!(handler.matches("/test", &Method::GET)); // 直接マッチ
        assert!(!handler.matches("/test/extra", &Method::GET)); // 部分マッチではない
        assert!(!handler.matches("/prefix/test", &Method::GET)); // 部分マッチではない
        assert!(!handler.matches("test", &Method::GET)); // パスが/で始まらない場合
        
        // 部分アンカーのケース
        let handler2 = get("^/partial", test_get_handler);
        let handler3 = get("/partial$", test_get_handler);
        
        // どちらも自動で^...$が追加されることを確認
        assert!(handler2.matches("/partial", &Method::GET));
        assert!(!handler2.matches("/partial/extra", &Method::GET));
        
        assert!(handler3.matches("/partial", &Method::GET));
        assert!(!handler3.matches("/prefix/partial", &Method::GET));
    }
    
    #[tokio::test]
    async fn test_try_new_api() {
        // try_new APIの動作テスト
        let result = try_get("/api/test", test_get_handler);
        assert!(result.is_ok());
        
        let handler = result.unwrap();
        assert!(handler.matches("/api/test", &Method::GET));
        
        // 空のパターンはエラー
        let result = try_get("", test_get_handler);
        assert!(result.is_err());
        
        // 非同期版もテスト
        let result = try_async_get("/async/test", test_async_get_handler);
        assert!(result.is_ok());
        
        let result = try_async_get("", test_async_get_handler);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_content_type_accept_plus_json() {
        // POSTハンドラー
        let handler = post("/plus-json", test_post_handler);

        // JSONボディと `application/ld+json; charset=utf-8` を付与
        let body = serde_json::to_vec(&TestRequest { name: "john".into(), value: 7 }).unwrap();
        let req = Request::new(Method::POST, "/plus-json".to_string())
            .with_header("Content-Type", "application/ld+json; charset=utf-8")
            .with_body(body);

        let res = handler.handle(req).await.expect("handler should accept +json");
        assert_eq!(res.status, 200);
    }

    #[tokio::test]
    async fn test_content_type_accept_json_seq() {
        // application/json-seq も許容リストに含める
        let handler = post("/json-seq", test_post_handler);

        let body = serde_json::to_vec(&TestRequest { name: "seq".into(), value: 3 }).unwrap();
        let req = Request::new(Method::POST, "/json-seq".to_string())
            .with_header("Content-Type", "application/json-seq")
            .with_body(body);

        let res = handler.handle(req).await.expect("handler should accept application/json-seq");
        assert_eq!(res.status, 200);
    }

    #[tokio::test]
    async fn test_content_type_reject_non_json() {
        // POSTハンドラー
        let handler = post("/reject", test_post_handler);

        // JSONボディだが `text/plain` は非許容
        let body = serde_json::to_vec(&TestRequest { name: "doe".into(), value: 1 }).unwrap();
        let req = Request::new(Method::POST, "/reject".to_string())
            .with_header("Content-Type", "text/plain")
            .with_body(body);

        let err = handler.handle(req).await.expect_err("handler should reject non-json content-type");
        match err {
            Error::InvalidRequestBody(msg) => {
                assert!(msg.contains("Unsupported Content-Type: text/plain"));
                assert!(msg.contains("expected application/json or *+json"));
            }
            e => panic!("unexpected error variant: {:?}", e),
        }
    }

    #[tokio::test]
    async fn test_content_type_header_case_insensitive() {
        // POSTハンドラー
        let handler = post("/case-insensitive", test_post_handler);

        // ヘッダー名を小文字で指定
        let body = serde_json::to_vec(&TestRequest { name: "case".into(), value: 2 }).unwrap();
        let req = Request::new(Method::POST, "/case-insensitive".to_string())
            .with_header("content-type", "application/json; charset=utf-8")
            .with_body(body);

        let res = handler.handle(req).await.expect("header lookup should be case-insensitive");
        assert_eq!(res.status, 200);
    }

    #[tokio::test]
    async fn test_empty_body_skips_validation_for_get() {
        // GETハンドラー（T=()）: 空ボディ（長さ0）ならパースも検証もスキップ
        let handler = get("/empty", test_get_handler);
        let req = Request::new(Method::GET, "/empty".to_string())
            .with_header("Content-Type", "text/plain") // 本来非対応だが空ボディなら影響しない
            .with_body(Vec::new()); // 空ボディ

        let res = handler.handle(req).await.expect("empty body should be ignored for GET");
        assert_eq!(res.status, 200);
    }

    #[tokio::test]
    async fn test_empty_body_treated_as_missing_for_post() {
        // POSTハンドラー: 空ボディ（長さ0）はMissing request bodyとしてエラー
        let handler = post("/empty-post", test_post_handler);
        let req = Request::new(Method::POST, "/empty-post".to_string())
            .with_header("Content-Type", "application/json")
            .with_body(Vec::new()); // 空ボディ

        let err = handler.handle(req).await.expect_err("empty body should be treated as missing for POST");
        match err {
            Error::InvalidRequestBody(msg) => assert!(msg.contains("Missing request body")),
            e => panic!("unexpected error variant: {:?}", e),
        }
    }
}
