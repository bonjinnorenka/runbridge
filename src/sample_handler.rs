//! サンプルハンドラの実装
//!
//! テスト用の簡単なハンドラを実装しています。

use async_trait::async_trait;
use log::info;
use serde_json;

use runbridge::common::{Handler, Method, Request, Response};
use runbridge::error::Error;

/// シンプルな Hello World ハンドラ
pub struct HelloHandler;

impl HelloHandler {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Handler for HelloHandler {
    fn matches(&self, path: &str, method: &Method) -> bool {
        path == "/" && *method == Method::GET
    }
    
    async fn handle(&self, _req: Request) -> Result<Response, Error> {
        info!("Handling Hello request");
        
        let mut response_data = serde_json::Map::new();
        
        response_data.insert("message".to_string(), 
            serde_json::Value::String("Hello from RunBridge CGI".to_string()));
        response_data.insert("version".to_string(), 
            serde_json::Value::String(env!("CARGO_PKG_VERSION").to_string()));
        
        let response_json = serde_json::Value::Object(response_data);
        
        Ok(Response::ok()
            .with_header("Content-Type", "application/json")
            .with_body(serde_json::to_vec(&response_json).unwrap()))
    }
}

/// リクエスト情報をエコーするハンドラ
pub struct EchoHandler;

impl EchoHandler {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Handler for EchoHandler {
    fn matches(&self, path: &str, method: &Method) -> bool {
        path == "/echo" && (*method == Method::GET || *method == Method::POST)
    }
    
    async fn handle(&self, req: Request) -> Result<Response, Error> {
        info!("Handling Echo request");
        
        let mut response_data = serde_json::Map::new();
        
        // 基本情報を追加
        response_data.insert("method".to_string(), serde_json::Value::String(format!("{}", req.method)));
        response_data.insert("path".to_string(), serde_json::Value::String(req.path.clone()));
        
        // クエリパラメータを追加
        for (key, value) in &req.query_params {
            response_data.insert(key.clone(), serde_json::Value::String(value.clone()));
        }
        
        // ヘッダーを追加
        for (key, value) in &req.headers {
            response_data.insert(key.clone(), serde_json::Value::String(value.clone()));
        }
        
        // ボディを追加
        if let Some(body) = &req.body {
            if let Ok(body_str) = String::from_utf8(body.clone()) {
                response_data.insert("body".to_string(), serde_json::Value::String(body_str.clone()));
                
                // コンテントタイプがJSONの場合、JSONとしてパースして中身も展開
                if req.headers.get("Content-Type").map_or(false, |ct| ct.contains("application/json")) {
                    if let Ok(json_value) = serde_json::from_str::<serde_json::Value>(&body_str) {
                        if let serde_json::Value::Object(map) = json_value {
                            for (key, value) in map {
                                response_data.insert(key, value);
                            }
                        }
                    }
                }
            } else {
                response_data.insert("body".to_string(), 
                    serde_json::Value::String(format!("<binary data of {} bytes>", body.len())));
            }
        }
        
        let response_json = serde_json::Value::Object(response_data);
        
        Ok(Response::ok()
            .with_header("Content-Type", "application/json")
            .with_body(serde_json::to_vec(&response_json).unwrap()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use runbridge::common::Method;
    
    #[tokio::test]
    async fn test_hello_handler() {
        let handler = HelloHandler::new();
        
        assert!(handler.matches("/", &Method::GET));
        assert!(!handler.matches("/", &Method::POST));
        assert!(!handler.matches("/other", &Method::GET));
        
        let req = Request::new(Method::GET, "/".to_string());
        let res = handler.handle(req).await.unwrap();
        
        assert_eq!(res.status, 200);
        assert_eq!(res.headers.get("Content-Type"), Some(&"application/json".to_string()));
        assert!(res.body.is_some());
    }
    
    #[tokio::test]
    async fn test_echo_handler() {
        let handler = EchoHandler::new();
        
        assert!(handler.matches("/echo", &Method::GET));
        assert!(handler.matches("/echo", &Method::POST));
        assert!(!handler.matches("/echo", &Method::PUT));
        assert!(!handler.matches("/other", &Method::GET));
        
        let req = Request::new(Method::POST, "/echo".to_string())
            .with_header("Content-Type", "application/json")
            .with_body(b"{\"name\":\"test\"}".to_vec());
            
        let res = handler.handle(req).await.unwrap();
        
        assert_eq!(res.status, 200);
        assert_eq!(res.headers.get("Content-Type"), Some(&"application/json".to_string()));
        assert!(res.body.is_some());
    }
} 