//! ハンドラーの実装

use std::marker::PhantomData;
use std::future::Future;
use async_trait::async_trait;
use serde::{Serialize, de::DeserializeOwned};
use log::{debug, info};
use regex::Regex;
use futures::future::{self, Ready};

use crate::common::{Handler, Method, Request, Response};
use crate::error::Error;

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
    pub fn new(method: Method, path_pattern: impl Into<String>, handler_fn: F) -> Self {
        let pattern = path_pattern.into();
        info!("Registering handler for {} with pattern: {}", method, pattern);
        Self {
            method,
            path_pattern: pattern,
            handler_fn,
            _request_type: PhantomData,
            _response_type: PhantomData,
        }
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
    pub fn new(method: Method, path_pattern: impl Into<String>, handler_fn: F) -> Self {
        let pattern = path_pattern.into();
        info!("Registering async handler for {} with pattern: {}", method, pattern);
        Self {
            method,
            path_pattern: pattern,
            handler_fn,
            _request_type: PhantomData,
            _response_type: PhantomData,
            _future_type: PhantomData,
        }
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

        // 正規表現パターンでパスをマッチング
        match Regex::new(&self.path_pattern) {
            Ok(re) => {
                let is_match = re.is_match(path);
                debug!("Path matching: {} against pattern {}: {}", path, self.path_pattern, is_match);
                is_match
            },
            Err(e) => {
                debug!("Invalid regex pattern: {} - {}", self.path_pattern, e);
                // 正規表現が無効な場合、単純な文字列比較を試みる
                path == self.path_pattern.trim_start_matches('^').trim_end_matches('$')
            }
        }
    }

    async fn handle(&self, req: Request) -> Result<Response, Error> {
        // リクエストボディをJSONとしてパース（存在する場合）
        let body_data = if req.body.is_some() {
            Some(req.json::<T>()?)
        } else {
            None
        };

        // ハンドラー関数を実行
        let result = (self.handler_fn)(req, body_data)?;

        // 結果がResponseの場合はそのまま返し、そうでなければJSONレスポンスに変換
        // Note: これは実際には動作しないコードで、次のステップで対応します
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

        // 正規表現パターンでパスをマッチング
        match Regex::new(&self.path_pattern) {
            Ok(re) => {
                let is_match = re.is_match(path);
                debug!("Path matching: {} against pattern {}: {}", path, self.path_pattern, is_match);
                is_match
            },
            Err(e) => {
                debug!("Invalid regex pattern: {} - {}", self.path_pattern, e);
                // 正規表現が無効な場合、単純な文字列比較を試みる
                path == self.path_pattern.trim_start_matches('^').trim_end_matches('$')
            }
        }
    }

    async fn handle(&self, req: Request) -> Result<Response, Error> {
        // リクエストボディをJSONとしてパース（存在する場合）
        let body_data = if req.body.is_some() {
            Some(req.json::<T>()?)
        } else {
            None
        };

        // 非同期ハンドラー関数を実行
        let result = (self.handler_fn)(req, body_data).await?;

        // 結果がResponseの場合はそのまま返し、そうでなければJSONレスポンスに変換
        // Note: これは実際には動作しないコードで、次のステップで対応します
        result.into_response()
    }
}

/// マクロでHTTPハンドラーを生成するための補助関数
pub fn get<F, R>(path: impl Into<String>, handler: F) -> RouteHandler<impl Fn(Request, Option<()>) -> Result<R, Error> + Send + Sync + 'static, (), R>
where
    F: Fn(Request) -> Result<R, Error> + Send + Sync + 'static,
    R: ResponseWrapper + Send + Sync + 'static,
{
    RouteHandler::new(Method::GET, path, move |req, _| handler(req))
}

/// 非同期GETハンドラーを作成
pub fn async_get<F, R, Fut>(path: impl Into<String>, handler: F) -> AsyncRouteHandler<impl Fn(Request, Option<()>) -> Fut + Send + Sync + 'static, (), R, Fut>
where
    F: Fn(Request) -> Fut + Send + Sync + 'static,
    R: ResponseWrapper + Send + Sync + 'static,
    Fut: Future<Output = Result<R, Error>> + Send + Sync + 'static,
{
    AsyncRouteHandler::new(Method::GET, path, move |req, _| handler(req))
}

/// POSTハンドラーを作成
pub fn post<F, T, R>(path: impl Into<String>, handler: F) -> RouteHandler<impl Fn(Request, Option<T>) -> Result<R, Error> + Send + Sync + 'static, T, R>
where
    F: Fn(Request, T) -> Result<R, Error> + Send + Sync + 'static,
    T: DeserializeOwned + Send + Sync + 'static,
    R: ResponseWrapper + Send + Sync + 'static,
{
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
    RouteHandler::new(Method::DELETE, path, move |req, _| handler(req))
}

/// 非同期DELETEハンドラーを作成
pub fn async_delete<F, R, Fut>(path: impl Into<String>, handler: F) -> AsyncRouteHandler<impl Fn(Request, Option<()>) -> Fut + Send + Sync + 'static, (), R, Fut>
where
    F: Fn(Request) -> Fut + Send + Sync + 'static,
    R: ResponseWrapper + Send + Sync + 'static,
    Fut: Future<Output = Result<R, Error>> + Send + Sync + 'static,
{
    AsyncRouteHandler::new(Method::DELETE, path, move |req, _| handler(req))
}

/// OPTIONSハンドラーを作成
pub fn options<F, R>(path: impl Into<String>, handler: F) -> RouteHandler<impl Fn(Request, Option<()>) -> Result<R, Error> + Send + Sync + 'static, (), R>
where
    F: Fn(Request) -> Result<R, Error> + Send + Sync + 'static,
    R: ResponseWrapper + Send + Sync + 'static,
{
    RouteHandler::new(Method::OPTIONS, path, move |req, _| handler(req))
}

/// 非同期OPTIONSハンドラーを作成
pub fn async_options<F, R, Fut>(path: impl Into<String>, handler: F) -> AsyncRouteHandler<impl Fn(Request, Option<()>) -> Fut + Send + Sync + 'static, (), R, Fut>
where
    F: Fn(Request) -> Fut + Send + Sync + 'static,
    R: ResponseWrapper + Send + Sync + 'static,
    Fut: Future<Output = Result<R, Error>> + Send + Sync + 'static,
{
    AsyncRouteHandler::new(Method::OPTIONS, path, move |req, _| handler(req))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Serialize, Deserialize};
    use std::sync::Arc;

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
}