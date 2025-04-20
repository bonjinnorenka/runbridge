//! 共通の抽象化レイヤーとトレイト定義

use std::collections::HashMap;
use std::fmt;
use async_trait::async_trait;
use serde::{Serialize, Deserialize};
use crate::error::Error;

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

/// ハンドラーの特性
#[async_trait]
pub trait Handler: Send + Sync {
    /// パスとメソッドがこのハンドラにマッチするかどうかを判定
    fn matches(&self, path: &str, method: &Method) -> bool;
    
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
} 