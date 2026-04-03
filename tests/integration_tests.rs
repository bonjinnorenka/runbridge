//! インテグレーションテスト

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use runbridge::{
        common::StatusCode,
        common::{Middleware, Next, Request, Response},
        error::Error,
        error_handler, handler, Cors, FixedFileOptions, FromRequestParts, Handler, Method, Router,
        RunBridge,
    };
    use serde::{Deserialize, Serialize};
    use std::fs;
    use std::path::PathBuf;
    use std::process;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;

    static NEXT_TEST_ID: AtomicU64 = AtomicU64::new(0);

    fn unique_temp_path(name: &str) -> PathBuf {
        let id = NEXT_TEST_ID.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "runbridge_integration_{}_{}_{}",
            process::id(),
            id,
            name
        ))
    }

    fn write_temp_file(name: &str, body: &[u8]) -> PathBuf {
        let path = unique_temp_path(name);
        fs::write(&path, body).expect("temp file must be written");
        path
    }

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
    async fn test_fixed_file_get_and_head() {
        let path = write_temp_file("favicon.ico", &[0x00, 0x01, 0x02, 0xff]);
        let app = RunBridge::builder()
            .fixed_file("/favicon.ico", &path)
            .build();

        let get_response = app
            .handle_request(Request::new(Method::GET, "/favicon.ico".to_string()))
            .await;
        assert_eq!(get_response.status, 200);
        assert_eq!(
            get_response.headers.get("Content-Type"),
            Some("image/x-icon")
        );
        assert_eq!(
            get_response.body,
            Some(bytes::Bytes::from_static(&[0x00, 0x01, 0x02, 0xff]))
        );

        let head_response = app
            .handle_request(Request::new(Method::HEAD, "/favicon.ico".to_string()))
            .await;
        assert_eq!(head_response.status, 200);
        assert_eq!(
            head_response.headers.get("Content-Type"),
            Some("image/x-icon")
        );
        assert!(head_response.body.is_none());

        let _ = fs::remove_file(path);
    }

    #[tokio::test]
    async fn test_fixed_file_without_head_returns_405() {
        let path = write_temp_file("robots.txt", b"User-agent: *\nDisallow:");
        let app = RunBridge::builder()
            .fixed_file_with(
                "/robots.txt",
                &path,
                FixedFileOptions::new()
                    .cache_control("public, max-age=300")
                    .without_head(),
            )
            .build();

        let get_response = app
            .handle_request(Request::new(Method::GET, "/robots.txt".to_string()))
            .await;
        assert_eq!(get_response.status, 200);
        assert_eq!(
            get_response.headers.get("Cache-Control"),
            Some("public, max-age=300")
        );
        assert_eq!(
            get_response.body.as_deref(),
            Some(&b"User-agent: *\nDisallow:"[..])
        );

        let head_response = app
            .handle_request(Request::new(Method::HEAD, "/robots.txt".to_string()))
            .await;
        assert_eq!(head_response.status, 405);
        assert_eq!(head_response.headers.get("Allow"), Some("GET"));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn test_fixed_file_rejects_duplicate_get_route() {
        let path = write_temp_file("duplicate-favicon.ico", &[0x00, 0x01, 0x02, 0xff]);
        let err = match RunBridge::builder()
            .handler(handler::get("/favicon.ico", |_req: Request| Ok("ok")))
            .try_fixed_file("/favicon.ico", &path)
        {
            Ok(_) => panic!("duplicate GET route must be rejected"),
            Err(err) => err,
        };

        assert!(matches!(err, Error::ConfigurationError(_)));

        let _ = fs::remove_file(path);
    }

    #[tokio::test]
    async fn test_fixed_file_coexists_with_other_routes() {
        let path = write_temp_file("assetlinks.json", br#"{"relation":[]}"#);
        let app = RunBridge::builder()
            .fixed_file_with(
                "/.well-known/assetlinks.json",
                &path,
                FixedFileOptions::new()
                    .content_disposition("inline")
                    .header("X-Fixed-File", "yes"),
            )
            .handler(handler::get("/health", |_req: Request| Ok("ok")))
            .build();

        let fixed_file_response = app
            .handle_request(Request::new(
                Method::GET,
                "/.well-known/assetlinks.json".to_string(),
            ))
            .await;
        assert_eq!(fixed_file_response.status, 200);
        assert_eq!(
            fixed_file_response.headers.get("Content-Type"),
            Some("application/json")
        );
        assert_eq!(
            fixed_file_response.headers.get("Content-Disposition"),
            Some("inline")
        );
        assert_eq!(fixed_file_response.headers.get("X-Fixed-File"), Some("yes"));

        let health_response = app
            .handle_request(Request::new(Method::GET, "/health".to_string()))
            .await;
        assert_eq!(health_response.status, 200);
        assert_eq!(health_response.body.as_deref(), Some(&b"\"ok\""[..]));

        let _ = fs::remove_file(path);
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

    struct ErrorMiddleware;

    #[async_trait]
    impl Middleware for ErrorMiddleware {
        async fn handle(&self, _req: Request, _next: Next<'_>) -> Result<Response, Error> {
            Err(Error::AuthenticationError("denied".to_string()))
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
    async fn test_cors_actual_request_preserves_headers_on_handler_error() {
        let cors = Cors::new()
            .allow_origin("https://example.com")
            .allow_methods([Method::GET]);

        let app = RunBridge::builder()
            .middleware(cors)
            .handler(handler::get(
                "/boom",
                |_req: Request| -> Result<Response, Error> {
                    Err(Error::InternalServerError("db failed".to_string()))
                },
            ))
            .build();

        let response = app
            .handle_request(
                Request::new(Method::GET, "/boom".to_string())
                    .with_header("Origin", "https://example.com"),
            )
            .await;

        assert_eq!(response.status, 500);
        assert_eq!(
            response.headers.get("Access-Control-Allow-Origin"),
            Some("https://example.com")
        );
        assert_eq!(response.headers.get_all("Vary"), vec!["Origin"]);
        assert_eq!(
            String::from_utf8(response.body.unwrap().to_vec()).unwrap(),
            "Internal Server Error"
        );
    }

    #[tokio::test]
    async fn test_cors_actual_request_preserves_headers_with_custom_error_handler() {
        let cors = Cors::new()
            .allow_origin("https://example.com")
            .allow_methods([Method::GET]);

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
            .middleware(cors)
            .handler(handler::get(
                "/boom",
                |_req: Request| -> Result<Response, Error> {
                    Err(Error::InternalServerError("db failed".to_string()))
                },
            ))
            .build();

        let response = app
            .handle_request(
                Request::new(Method::GET, "/boom".to_string())
                    .with_header("Origin", "https://example.com"),
            )
            .await;

        assert_eq!(response.status, 500);
        assert_eq!(
            response.headers.get("Access-Control-Allow-Origin"),
            Some("https://example.com")
        );
        assert_eq!(
            response.headers.get("Content-Type"),
            Some("application/json")
        );

        let body: serde_json::Value =
            serde_json::from_slice(response.body.as_ref().unwrap()).unwrap();
        assert_eq!(body["path"], "/boom");
        assert_eq!(body["status"], 500);
    }

    #[tokio::test]
    async fn test_cors_actual_request_preserves_headers_on_middleware_error() {
        let cors = Cors::new()
            .allow_origin("https://example.com")
            .allow_methods([Method::GET]);

        let app = RunBridge::builder()
            .middleware(cors)
            .middleware(ErrorMiddleware)
            .handler(handler::get("/cors", |_req: Request| {
                Ok(Response::ok().with_body("ok".as_bytes().to_vec()))
            }))
            .build();

        let response = app
            .handle_request(
                Request::new(Method::GET, "/cors".to_string())
                    .with_header("Origin", "https://example.com"),
            )
            .await;

        assert_eq!(response.status, 401);
        assert_eq!(
            response.headers.get("Access-Control-Allow-Origin"),
            Some("https://example.com")
        );
        assert_eq!(response.headers.get_all("Vary"), vec!["Origin"]);
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

    #[tokio::test]
    async fn test_router_scoped_middleware_applies_to_not_found_and_method_not_allowed() {
        let api_router = Router::new()
            .middleware(ShortCircuitMiddleware)
            .route(handler::get("/items", |_req: Request| {
                Ok(Response::ok().with_body("ok".as_bytes().to_vec()))
            }));

        let app = RunBridge::builder().nest("/api", api_router).build();

        let not_found = app
            .handle_request(Request::new(Method::GET, "/api/missing".to_string()))
            .await;
        assert_eq!(not_found.status, 401);
        assert_eq!(
            String::from_utf8(not_found.body.unwrap().to_vec()).unwrap(),
            "blocked"
        );

        let method_not_allowed = app
            .handle_request(Request::new(Method::POST, "/api/items".to_string()))
            .await;
        assert_eq!(method_not_allowed.status, 401);
        assert_eq!(
            String::from_utf8(method_not_allowed.body.unwrap().to_vec()).unwrap(),
            "blocked"
        );
    }

    #[tokio::test]
    async fn test_router_scoped_post_processing_applies_to_not_found_and_method_not_allowed() {
        let api_router = Router::new()
            .middleware(HeaderMiddleware { value: "api" })
            .route(handler::get("/items", |_req: Request| {
                Ok(Response::ok().with_body("ok".as_bytes().to_vec()))
            }));

        let app = RunBridge::builder().nest("/api", api_router).build();

        let not_found = app
            .handle_request(Request::new(Method::GET, "/api/missing".to_string()))
            .await;
        assert_eq!(not_found.status, 404);
        assert_eq!(not_found.headers.get_all("X-Order"), vec!["api"]);

        let method_not_allowed = app
            .handle_request(Request::new(Method::POST, "/api/items".to_string()))
            .await;
        assert_eq!(method_not_allowed.status, 405);
        assert_eq!(method_not_allowed.headers.get_all("X-Order"), vec!["api"]);
    }

    #[tokio::test]
    async fn test_router_scoped_middleware_applies_to_fallback() {
        let api_router = Router::new().middleware(HeaderMiddleware { value: "api" });

        let app = RunBridge::builder()
            .nest("/api", api_router)
            .fallback(handler::fallback(|req: Request| {
                Ok(Response::not_found().with_body(format!("fallback: {}", req.path).into_bytes()))
            }))
            .build();

        let response = app
            .handle_request(Request::new(Method::GET, "/api/missing".to_string()))
            .await;

        assert_eq!(response.status, 404);
        assert_eq!(response.headers.get_all("X-Order"), vec!["api"]);
        assert_eq!(
            String::from_utf8(response.body.unwrap().to_vec()).unwrap(),
            "fallback: /api/missing"
        );
    }

    #[tokio::test]
    async fn test_router_scoped_middleware_respects_prefix_boundaries() {
        let app = RunBridge::builder()
            .nest("/api", Router::new().middleware(ShortCircuitMiddleware))
            .build();

        let response = app
            .handle_request(Request::new(Method::GET, "/api2/missing".to_string()))
            .await;

        assert_eq!(response.status, 404);
        assert_eq!(
            String::from_utf8(response.body.unwrap().to_vec()).unwrap(),
            "Not Found"
        );
    }

    #[tokio::test]
    async fn test_nested_router_scoped_post_processing_order_for_not_found() {
        let admin_router = Router::new().middleware(HeaderMiddleware { value: "admin" });
        let api_router = Router::new()
            .middleware(HeaderMiddleware { value: "api" })
            .nest("/admin", admin_router);

        let app = RunBridge::builder().nest("/api", api_router).build();

        let response = app
            .handle_request(Request::new(Method::GET, "/api/admin/missing".to_string()))
            .await;

        assert_eq!(response.status, 404);
        assert_eq!(response.headers.get_all("X-Order"), vec!["admin", "api"]);
    }
}
