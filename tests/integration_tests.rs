//! インテグレーションテスト

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use runbridge::{
        common::{Middleware, Next, Request, Response},
        error::Error,
        handler, FromRequestParts, Handler, Method, RunBridge,
    };
    use serde::{Deserialize, Serialize};
    use std::sync::Arc;

    #[derive(Serialize, Deserialize, Debug, PartialEq)]
    struct ItemRequest {
        name: String,
        description: Option<String>,
    }

    #[derive(Serialize, Deserialize, Debug, PartialEq)]
    struct ItemResponse {
        id: String,
        name: String,
        description: Option<String>,
        created_at: String,
    }

    fn get_item_handler(req: Request) -> Result<ItemResponse, Error> {
        let id = req
            .path_params
            .get("id")
            .cloned()
            .unwrap_or_else(|| "unknown".to_string());

        Ok(ItemResponse {
            id,
            name: "Test Item".to_string(),
            description: Some("This is a test item".to_string()),
            created_at: "2023-01-01T00:00:00Z".to_string(),
        })
    }

    fn create_item_handler(_req: Request, item: ItemRequest) -> Result<ItemResponse, Error> {
        Ok(ItemResponse {
            id: "new_item_123".to_string(),
            name: item.name,
            description: item.description,
            created_at: "2023-01-01T00:00:00Z".to_string(),
        })
    }

    #[tokio::test]
    async fn test_app_routing() {
        let app = RunBridge::builder()
            .handler(handler::get("/items/{id}", get_item_handler))
            .handler(handler::post("/items", create_item_handler))
            .build();

        let get_req = Request::new(Method::GET, "/items/123".to_string());
        let get_result = app.handle_request(get_req).await;
        assert_eq!(get_result.status, 200);
        let body_str = String::from_utf8(get_result.body.unwrap().to_vec()).unwrap();
        let response: ItemResponse = serde_json::from_str(&body_str).unwrap();
        assert_eq!(response.id, "123");

        let post_req = Request::new(Method::POST, "/items".to_string())
            .with_header("Content-Type", "application/json")
            .with_body(
                serde_json::to_vec(&ItemRequest {
                    name: "New Item".to_string(),
                    description: Some("This is a new item".to_string()),
                })
                .unwrap(),
            );

        let post_result = app.handle_request(post_req).await;
        assert_eq!(post_result.status, 200);
        let body_str = String::from_utf8(post_result.body.unwrap().to_vec()).unwrap();
        let response: ItemResponse = serde_json::from_str(&body_str).unwrap();
        assert_eq!(response.name, "New Item");
        assert_eq!(response.id, "new_item_123");
    }

    #[tokio::test]
    async fn test_method_not_allowed_and_allow_header() {
        let app = RunBridge::builder()
            .handler(handler::get("/items/{id}", get_item_handler))
            .build();

        let req = Request::new(Method::POST, "/items/123".to_string());
        let response = app.handle_request(req).await;

        assert_eq!(response.status, 405);
        assert_eq!(response.headers.get("Allow"), Some("GET"));
    }

    #[tokio::test]
    async fn test_fallback_for_not_found_only() {
        let app = RunBridge::builder()
            .handler(handler::get("/items/{id}", get_item_handler))
            .fallback(handler::fallback(|req: Request| {
                Ok(Response::not_found().with_body(format!("fallback: {}", req.path).into_bytes()))
            }))
            .build();

        let not_found = app
            .handle_request(Request::new(Method::GET, "/missing".to_string()))
            .await;
        assert_eq!(not_found.status, 404);
        assert_eq!(
            String::from_utf8(not_found.body.unwrap().to_vec()).unwrap(),
            "fallback: /missing"
        );

        let method_not_allowed = app
            .handle_request(Request::new(Method::POST, "/items/123".to_string()))
            .await;
        assert_eq!(method_not_allowed.status, 405);
    }

    struct TestMiddleware {
        name: String,
    }

    #[async_trait]
    impl Middleware for TestMiddleware {
        async fn handle(&self, req: Request, next: Next<'_>) -> Result<Response, Error> {
            let mut response = next.run(req).await?;
            response
                .headers
                .append("X-Middleware-Response", self.name.clone());
            Ok(response)
        }
    }

    struct ShortCircuitMiddleware;

    #[async_trait]
    impl Middleware for ShortCircuitMiddleware {
        async fn handle(&self, _req: Request, _next: Next<'_>) -> Result<Response, Error> {
            Ok(Response::unauthorized().with_body("blocked".as_bytes().to_vec()))
        }
    }

    #[tokio::test]
    async fn test_middleware_chain() {
        let app = RunBridge::builder()
            .middleware(TestMiddleware {
                name: "Test1".to_string(),
            })
            .middleware(TestMiddleware {
                name: "Test2".to_string(),
            })
            .handler(handler::get("/test", |_| {
                Ok(Response::ok().with_body("ok".as_bytes().to_vec()))
            }))
            .build();

        let response = app
            .handle_request(Request::new(Method::GET, "/test".to_string()))
            .await;

        assert_eq!(response.status, 200);
        assert_eq!(
            response.headers.get_all("X-Middleware-Response"),
            vec!["Test2", "Test1"]
        );
    }

    #[tokio::test]
    async fn test_middleware_short_circuit() {
        let app = RunBridge::builder()
            .middleware(ShortCircuitMiddleware)
            .handler(handler::get("/test", |_| {
                Ok(Response::ok().with_body("ok".as_bytes().to_vec()))
            }))
            .build();

        let response = app
            .handle_request(Request::new(Method::GET, "/test".to_string()))
            .await;
        assert_eq!(response.status, 401);
        assert_eq!(
            String::from_utf8(response.body.unwrap().to_vec()).unwrap(),
            "blocked"
        );
    }

    #[derive(Clone)]
    struct AppState {
        prefix: String,
    }

    struct StateEchoHandler;

    #[async_trait]
    impl Handler for StateEchoHandler {
        async fn handle(&self, req: Request) -> Result<Response, Error> {
            let parts = handler::RequestParts::from(&req);
            let state = handler::State::<AppState>::from_request_parts(&parts)
                .await
                .map_err(|err| Error::InternalServerError(err.message().to_string()))?;
            Ok(Response::ok().with_body(format!("{}{}", state.prefix, req.path).into_bytes()))
        }
    }

    #[tokio::test]
    async fn test_state_extractor_via_builder_state() {
        let app = RunBridge::builder()
            .state(Arc::new(AppState {
                prefix: "state:".to_string(),
            }))
            .handler(handler::route(Method::GET, "/state", StateEchoHandler))
            .build();

        let response = app
            .handle_request(Request::new(Method::GET, "/state".to_string()))
            .await;

        assert_eq!(response.status, 200);
        assert_eq!(
            String::from_utf8(response.body.unwrap().to_vec()).unwrap(),
            "state:/state"
        );
    }
}
