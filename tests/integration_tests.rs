//! インテグレーションテスト

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use runbridge::{
        common::StatusCode,
        common::{Middleware, Next, Request, Response},
        error::Error,
        error_handler, handler, Cors, FromRequestParts, Handler, Method, Router, RunBridge,
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

    struct HeaderMiddleware {
        value: &'static str,
    }

    #[async_trait]
    impl Middleware for HeaderMiddleware {
        async fn handle(&self, req: Request, next: Next<'_>) -> Result<Response, Error> {
            let mut response = next.run(req).await?;
            response.headers.append("X-Order", self.value);
            Ok(response)
        }
    }

    #[tokio::test]
    async fn test_router_nest_applies_prefix_and_scoped_middleware() {
        let api_router = Router::new()
            .middleware(HeaderMiddleware { value: "api" })
            .route(handler::get("/users/{id}", |req: Request| {
                Ok(Response::ok().with_body(
                    req.path_params
                        .get("id")
                        .cloned()
                        .unwrap_or_default()
                        .into_bytes(),
                ))
            }));

        let app = RunBridge::builder()
            .middleware(HeaderMiddleware { value: "app" })
            .nest("/api", api_router)
            .handler(handler::get("/health", |_req: Request| {
                Ok(Response::ok().with_body("ok".as_bytes().to_vec()))
            }))
            .build();

        let api_response = app
            .handle_request(Request::new(Method::GET, "/api/users/42".to_string()))
            .await;
        assert_eq!(api_response.status, 200);
        assert_eq!(api_response.headers.get_all("X-Order"), vec!["api", "app"]);
        assert_eq!(
            String::from_utf8(api_response.body.unwrap().to_vec()).unwrap(),
            "42"
        );

        let health_response = app
            .handle_request(Request::new(Method::GET, "/health".to_string()))
            .await;
        assert_eq!(health_response.status, 200);
        assert_eq!(health_response.headers.get_all("X-Order"), vec!["app"]);
    }

    #[tokio::test]
    async fn test_router_merge_combines_routes_and_ignores_duplicates() {
        let merged = Router::new()
            .route(handler::get("/one", |_req: Request| Ok("one")))
            .merge(
                Router::new()
                    .route(handler::get("/two", |_req: Request| Ok("two")))
                    .route(handler::get("/one", |_req: Request| Ok("duplicate"))),
            );

        let app = RunBridge::builder().router(merged).build();

        let one = app
            .handle_request(Request::new(Method::GET, "/one".to_string()))
            .await;
        assert_eq!(
            String::from_utf8(one.body.unwrap().to_vec()).unwrap(),
            "\"one\""
        );

        let two = app
            .handle_request(Request::new(Method::GET, "/two".to_string()))
            .await;
        assert_eq!(
            String::from_utf8(two.body.unwrap().to_vec()).unwrap(),
            "\"two\""
        );
    }

    #[tokio::test]
    async fn test_route_layer_runs_inside_global_middleware() {
        let app = RunBridge::builder()
            .middleware(HeaderMiddleware { value: "app" })
            .handler(
                handler::get("/layer", |_req: Request| {
                    Ok(Response::ok().with_body("ok".as_bytes().to_vec()))
                })
                .layer(HeaderMiddleware { value: "route" }),
            )
            .build();

        let response = app
            .handle_request(Request::new(Method::GET, "/layer".to_string()))
            .await;

        assert_eq!(response.headers.get_all("X-Order"), vec!["route", "app"]);
    }

    #[tokio::test]
    async fn test_patch_and_head_routes_and_allow_header() {
        let app = RunBridge::builder()
            .handler(handler::patch(
                "/items",
                |_req: Request, body: ItemRequest| {
                    Ok(Response::ok().with_body(body.name.into_bytes()))
                },
            ))
            .handler(handler::head("/items", |_req: Request| {
                Ok(Response::ok()
                    .with_header("X-Head", "yes")
                    .with_body("hidden".as_bytes().to_vec()))
            }))
            .build();

        let patch_response = app
            .handle_request(
                Request::new(Method::PATCH, "/items".to_string())
                    .with_header("Content-Type", "application/json")
                    .with_body(
                        serde_json::to_vec(&ItemRequest {
                            name: "patched".to_string(),
                            description: None,
                        })
                        .unwrap(),
                    ),
            )
            .await;
        assert_eq!(patch_response.status, 200);
        assert_eq!(
            String::from_utf8(patch_response.body.unwrap().to_vec()).unwrap(),
            "patched"
        );

        let head_response = app
            .handle_request(Request::new(Method::HEAD, "/items".to_string()))
            .await;
        assert_eq!(head_response.status, 200);
        assert_eq!(head_response.headers.get("X-Head"), Some("yes"));
        assert!(head_response.body.is_none());

        let method_not_allowed = app
            .handle_request(Request::new(Method::POST, "/items".to_string()))
            .await;
        assert_eq!(method_not_allowed.status, 405);
        assert_eq!(method_not_allowed.headers.get("Allow"), Some("PATCH, HEAD"));
    }

    #[tokio::test]
    async fn test_head_suppresses_body_for_error_handler_and_fallback() {
        let app = RunBridge::builder()
            .error_handler(error_handler(|req, err| {
                let path = req.path.clone();
                let status = err.status_code();
                async move {
                    Response::with_status(StatusCode::InternalServerError)
                        .with_header("Content-Type", "application/json")
                        .with_body(
                            serde_json::to_vec(&serde_json::json!({
                                "path": path,
                                "status": status,
                            }))
                            .unwrap(),
                        )
                }
            }))
            .handler(handler::head(
                "/error",
                |_req: Request| -> Result<Response, Error> {
                    Err(Error::InternalServerError("boom".to_string()))
                },
            ))
            .fallback(handler::fallback(|_req: Request| {
                Ok(Response::not_found().with_body("fallback".as_bytes().to_vec()))
            }))
            .build();

        let error_response = app
            .handle_request(Request::new(Method::HEAD, "/error".to_string()))
            .await;
        assert_eq!(error_response.status, 500);
        assert!(error_response.body.is_none());

        let fallback_response = app
            .handle_request(Request::new(Method::HEAD, "/missing".to_string()))
            .await;
        assert_eq!(fallback_response.status, 404);
        assert!(fallback_response.body.is_none());
    }

    #[tokio::test]
    async fn test_custom_error_handler_applies_only_to_handler_errors() {
        let app = RunBridge::builder()
            .error_handler(error_handler(|req, err| {
                let path = req.path.clone();
                let method = req.method.to_string();
                let status = err.status_code();
                async move {
                    Response::with_status(StatusCode::InternalServerError)
                        .with_header("Content-Type", "application/json")
                        .with_body(
                            serde_json::to_vec(&serde_json::json!({
                                "path": path,
                                "method": method,
                                "status": status,
                            }))
                            .unwrap(),
                        )
                }
            }))
            .handler(handler::get(
                "/boom",
                |_req: Request| -> Result<Response, Error> {
                    Err(Error::InternalServerError("db failed".to_string()))
                },
            ))
            .build();

        let error_response = app
            .handle_request(Request::new(Method::GET, "/boom".to_string()))
            .await;
        assert_eq!(error_response.status, 500);
        assert_eq!(
            error_response.headers.get("Content-Type"),
            Some("application/json")
        );

        let body: serde_json::Value =
            serde_json::from_slice(error_response.body.as_ref().unwrap()).unwrap();
        assert_eq!(body["path"], "/boom");
        assert_eq!(body["method"], "GET");
        assert_eq!(body["status"], 500);

        let not_found = app
            .handle_request(Request::new(Method::GET, "/missing".to_string()))
            .await;
        assert_eq!(not_found.status, 404);
        assert_eq!(not_found.headers.get("Content-Type"), Some("text/plain"));

        let method_not_allowed = app
            .handle_request(Request::new(Method::POST, "/boom".to_string()))
            .await;
        assert_eq!(method_not_allowed.status, 405);
        assert_eq!(
            String::from_utf8(method_not_allowed.body.unwrap().to_vec()).unwrap(),
            "Method Not Allowed"
        );
    }

    #[tokio::test]
    async fn test_default_error_handler_remains_plain_text() {
        let app = RunBridge::builder()
            .handler(handler::get(
                "/boom",
                |_req: Request| -> Result<Response, Error> {
                    Err(Error::InternalServerError("db failed".to_string()))
                },
            ))
            .build();

        let response = app
            .handle_request(Request::new(Method::GET, "/boom".to_string()))
            .await;
        assert_eq!(response.status, 500);
        assert_eq!(response.headers.get("Content-Type"), Some("text/plain"));
        assert_eq!(
            String::from_utf8(response.body.unwrap().to_vec()).unwrap(),
            "Internal Server Error"
        );
    }

    #[tokio::test]
    async fn test_cors_actual_and_preflight_requests() {
        let cors = Cors::new()
            .allow_origin("https://example.com")
            .allow_methods([Method::GET, Method::PATCH])
            .allow_headers(["Content-Type", "X-Token"])
            .expose_headers(["X-Trace-Id"])
            .max_age(600);

        let app = RunBridge::builder()
            .middleware(cors)
            .handler(handler::get("/cors", |_req: Request| {
                Ok(Response::ok().with_body("ok".as_bytes().to_vec()))
            }))
            .build();

        let actual = app
            .handle_request(
                Request::new(Method::GET, "/cors".to_string())
                    .with_header("Origin", "https://example.com"),
            )
            .await;
        assert_eq!(actual.status, 200);
        assert_eq!(
            actual.headers.get("Access-Control-Allow-Origin"),
            Some("https://example.com")
        );
        assert_eq!(
            actual.headers.get("Access-Control-Expose-Headers"),
            Some("X-Trace-Id")
        );
        assert_eq!(actual.headers.get_all("Vary"), vec!["Origin"]);

        let preflight = app
            .handle_request(
                Request::new(Method::OPTIONS, "/cors".to_string())
                    .with_header("Origin", "https://example.com")
                    .with_header("Access-Control-Request-Method", "PATCH")
                    .with_header("Access-Control-Request-Headers", "Content-Type, X-Token"),
            )
            .await;
        assert_eq!(preflight.status, 204);
        assert_eq!(
            preflight.headers.get("Access-Control-Allow-Origin"),
            Some("https://example.com")
        );
        assert_eq!(
            preflight.headers.get("Access-Control-Allow-Methods"),
            Some("GET, PATCH")
        );
        assert_eq!(
            preflight.headers.get("Access-Control-Allow-Headers"),
            Some("Content-Type, X-Token")
        );
        assert_eq!(preflight.headers.get("Access-Control-Max-Age"), Some("600"));
        assert_eq!(
            preflight.headers.get_all("Vary"),
            vec![
                "Origin",
                "Access-Control-Request-Method",
                "Access-Control-Request-Headers"
            ]
        );
    }

    #[tokio::test]
    async fn test_cors_disallowed_origin_and_invalid_config() {
        let cors = Cors::new()
            .allow_origin("https://allowed.example")
            .allow_methods([Method::GET]);

        let app = RunBridge::builder()
            .middleware(cors)
            .handler(handler::get("/cors", |_req: Request| {
                Ok(Response::ok().with_body("ok".as_bytes().to_vec()))
            }))
            .build();

        let response = app
            .handle_request(
                Request::new(Method::GET, "/cors".to_string())
                    .with_header("Origin", "https://denied.example"),
            )
            .await;
        assert_eq!(response.status, 200);
        assert!(response
            .headers
            .get("Access-Control-Allow-Origin")
            .is_none());

        let invalid = Cors::new().allow_any_origin().allow_credentials(true);
        assert!(invalid.try_validate().is_err());
    }
}
