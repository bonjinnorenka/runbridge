//! サンプルハンドラの実装

use async_trait::async_trait;
use log::info;

use runbridge::common::{Handler, Request, Response};
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
    async fn handle(&self, _req: Request) -> Result<Response, Error> {
        info!("Handling Hello request");

        let response_json = serde_json::json!({
            "message": "Hello from RunBridge CGI",
            "version": env!("CARGO_PKG_VERSION"),
        });

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
    async fn handle(&self, req: Request) -> Result<Response, Error> {
        info!("Handling Echo request");

        let mut response_data = serde_json::Map::new();
        response_data.insert(
            "method".to_string(),
            serde_json::Value::String(req.method.to_string()),
        );
        response_data.insert(
            "path".to_string(),
            serde_json::Value::String(req.path.clone()),
        );

        for (key, value) in &req.query {
            response_data.insert(
                key.to_string(),
                serde_json::Value::String(value.to_string()),
            );
        }

        for (key, value) in &req.headers {
            response_data.insert(
                key.to_ascii_lowercase(),
                serde_json::Value::String(value.to_string()),
            );
        }

        if let Some(body) = &req.body {
            if let Ok(body_str) = String::from_utf8(body.to_vec()) {
                response_data.insert(
                    "body".to_string(),
                    serde_json::Value::String(body_str.clone()),
                );

                if req
                    .headers
                    .get("content-type")
                    .is_some_and(|ct| ct.contains("application/json"))
                {
                    if let Ok(serde_json::Value::Object(map)) =
                        serde_json::from_str::<serde_json::Value>(&body_str)
                    {
                        for (key, value) in map {
                            response_data.insert(key, value);
                        }
                    }
                }
            } else {
                response_data.insert(
                    "body".to_string(),
                    serde_json::Value::String(format!("<binary data of {} bytes>", body.len())),
                );
            }
        }

        Ok(Response::ok()
            .with_header("Content-Type", "application/json")
            .with_body(serde_json::to_vec(&serde_json::Value::Object(response_data)).unwrap()))
    }
}

/// パニックテスト用ハンドラ
pub struct PanicHandler;

impl PanicHandler {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Handler for PanicHandler {
    async fn handle(&self, _req: Request) -> Result<Response, Error> {
        info!("Handling Panic request - this will panic!");
        panic!("Test panic from handler");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use runbridge::{handler, Method, RunBridgeBuilder};

    #[tokio::test]
    async fn test_hello_handler() {
        let handler = HelloHandler::new();
        let req = Request::new(Method::GET, "/".to_string());
        let res = handler.handle(req).await.unwrap();

        assert_eq!(res.status, 200);
        assert_eq!(res.headers.get("Content-Type"), Some("application/json"));
        assert!(res.body.is_some());
    }

    #[tokio::test]
    async fn test_echo_handler() {
        let handler = EchoHandler::new();
        let req = Request::new(Method::POST, "/echo".to_string())
            .with_header("Content-Type", "application/json")
            .with_body(b"{\"name\":\"test\"}".to_vec());

        let res = handler.handle(req).await.unwrap();

        assert_eq!(res.status, 200);
        assert_eq!(res.headers.get("Content-Type"), Some("application/json"));
        assert!(res.body.is_some());
    }

    struct TestShallowHandler;

    #[async_trait]
    impl Handler for TestShallowHandler {
        async fn handle(&self, _req: Request) -> Result<Response, Error> {
            Ok(Response::ok().with_body(b"Shallow Handler Response".to_vec()))
        }
    }

    struct TestDeepHandler;

    #[async_trait]
    impl Handler for TestDeepHandler {
        async fn handle(&self, _req: Request) -> Result<Response, Error> {
            Ok(Response::ok().with_body(b"Deep Handler Response".to_vec()))
        }
    }

    #[tokio::test]
    async fn test_path_nesting_priority() {
        let app = RunBridgeBuilder::new()
            .handler(handler::route(Method::GET, "/test", TestShallowHandler))
            .handler(handler::route(Method::GET, "/test/deep", TestDeepHandler))
            .build();

        let req_deep = Request::new(Method::GET, "/test/deep".to_string());
        let res = app.handle_request(req_deep).await;
        assert_eq!(res.status, 200);
        assert_eq!(res.body.unwrap(), b"Deep Handler Response".to_vec());

        let req_shallow = Request::new(Method::GET, "/test".to_string());
        let res_shallow = app.handle_request(req_shallow).await;
        assert_eq!(res_shallow.status, 200);
        assert_eq!(
            res_shallow.body.unwrap(),
            b"Shallow Handler Response".to_vec()
        );
    }
}
