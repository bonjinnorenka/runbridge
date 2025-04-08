//! ハンドラーの実装

use std::marker::PhantomData;
use async_trait::async_trait;
use serde::{Serialize, de::DeserializeOwned};
use log::{debug, info};
use regex::Regex;

use crate::common::{Handler, Method, Request, Response};
use crate::error::Error;

/// ルートハンドラー
pub struct RouteHandler<F, T, R>
where
    F: Fn(Request, Option<T>) -> Result<R, Error> + Send + Sync + 'static,
    T: DeserializeOwned + Send + Sync + 'static,
    R: Serialize + Send + Sync + 'static,
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
    R: Serialize + Send + Sync + 'static,
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

#[async_trait]
impl<F, T, R> Handler for RouteHandler<F, T, R>
where
    F: Fn(Request, Option<T>) -> Result<R, Error> + Send + Sync + 'static,
    T: DeserializeOwned + Send + Sync + 'static,
    R: Serialize + Send + Sync + 'static,
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

        // 結果をJSONレスポンスに変換
        Response::ok().json(&result)
    }
}

/// マクロでHTTPハンドラーを生成するための補助関数
pub fn get<F, R>(path: impl Into<String>, handler: F) -> RouteHandler<impl Fn(Request, Option<()>) -> Result<R, Error> + Send + Sync + 'static, (), R>
where
    F: Fn(Request) -> Result<R, Error> + Send + Sync + 'static,
    R: Serialize + Send + Sync + 'static,
{
    RouteHandler::new(Method::GET, path, move |req, _| handler(req))
}

/// POSTハンドラーを作成
pub fn post<F, T, R>(path: impl Into<String>, handler: F) -> RouteHandler<impl Fn(Request, Option<T>) -> Result<R, Error> + Send + Sync + 'static, T, R>
where
    F: Fn(Request, T) -> Result<R, Error> + Send + Sync + 'static,
    T: DeserializeOwned + Send + Sync + 'static,
    R: Serialize + Send + Sync + 'static,
{
    RouteHandler::new(Method::POST, path, move |req, body_data| {
        if let Some(data) = body_data {
            handler(req, data)
        } else {
            Err(Error::InvalidRequestBody("Missing request body".to_string()))
        }
    })
}

/// PUTハンドラーを作成
pub fn put<F, T, R>(path: impl Into<String>, handler: F) -> RouteHandler<impl Fn(Request, Option<T>) -> Result<R, Error> + Send + Sync + 'static, T, R>
where
    F: Fn(Request, T) -> Result<R, Error> + Send + Sync + 'static,
    T: DeserializeOwned + Send + Sync + 'static,
    R: Serialize + Send + Sync + 'static,
{
    RouteHandler::new(Method::PUT, path, move |req, body_data| {
        if let Some(data) = body_data {
            handler(req, data)
        } else {
            Err(Error::InvalidRequestBody("Missing request body".to_string()))
        }
    })
}

/// DELETEハンドラーを作成
pub fn delete<F, R>(path: impl Into<String>, handler: F) -> RouteHandler<impl Fn(Request, Option<()>) -> Result<R, Error> + Send + Sync + 'static, (), R>
where
    F: Fn(Request) -> Result<R, Error> + Send + Sync + 'static,
    R: Serialize + Send + Sync + 'static,
{
    RouteHandler::new(Method::DELETE, path, move |req, _| handler(req))
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
}