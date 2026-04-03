use super::*;
use crate::common::{Handler, Method, Request, Response};
use crate::error::Error;
use serde::{Deserialize, Serialize};

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

#[derive(Deserialize, Debug, PartialEq)]
struct TestPathParams {
    id: u32,
}

#[derive(Deserialize, Debug, PartialEq)]
struct TestQueryParams {
    tag: Vec<String>,
    page: u32,
}

#[derive(Deserialize, Debug, PartialEq)]
struct SingleTagQueryParams {
    tag: Vec<String>,
}

#[derive(Deserialize, Debug, PartialEq)]
struct StringQueryParams {
    code: String,
    flag: String,
}

#[derive(Deserialize, Debug, PartialEq)]
struct StringPathParams {
    id: String,
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

#[tokio::test]
async fn test_get_route_matches() {
    let route = get("/test", test_get_handler);
    assert!(route.matches("/test", &Method::GET));
    assert!(!route.matches("/test", &Method::POST));
    assert!(!route.matches("/other", &Method::GET));
}

#[tokio::test]
async fn test_get_handler_execution() {
    let route = get("/test", test_get_handler);
    let req = Request::new(Method::GET, "/test".to_string());

    let result = route.handle(req).await.unwrap();
    assert_eq!(result.status, 200);

    let body_str = String::from_utf8(result.body.unwrap().to_vec()).unwrap();
    let response: TestResponse = serde_json::from_str(&body_str).unwrap();

    assert_eq!(response.message, "Hello from GET");
    assert_eq!(response.value, 42);
}

#[tokio::test]
async fn test_post_handler_execution() {
    let route = post("/users", test_post_handler);
    let test_data = TestRequest {
        name: "Test User".to_string(),
        value: 21,
    };

    let json_body = serde_json::to_vec(&test_data).unwrap();
    let req = Request::new(Method::POST, "/users".to_string())
        .with_header("Content-Type", "application/json; charset=utf-8")
        .with_body(json_body);

    let result = route.handle(req).await.unwrap();
    assert_eq!(result.status, 200);

    let body_str = String::from_utf8(result.body.unwrap().to_vec()).unwrap();
    let response: TestResponse = serde_json::from_str(&body_str).unwrap();

    assert_eq!(response.message, "Hello, Test User");
    assert_eq!(response.value, 42);
}

#[tokio::test]
async fn test_post_handler_missing_body_returns_400_response() {
    let route = post("/users", test_post_handler);
    let req = Request::new(Method::POST, "/users".to_string())
        .with_header("Content-Type", "application/json");

    let result = route.handle(req).await.unwrap();
    assert_eq!(result.status, 400);
}

#[tokio::test]
async fn test_plus_json_content_type_is_accepted() {
    let route = post("/users", test_post_handler);
    let test_data = TestRequest {
        name: "Test User".to_string(),
        value: 21,
    };

    let req = Request::new(Method::POST, "/users".to_string())
        .with_header("Content-Type", "application/ld+json; charset=utf-8")
        .with_body(serde_json::to_vec(&test_data).unwrap());

    let result = route.handle(req).await.unwrap();
    assert_eq!(result.status, 200);
}

#[tokio::test]
async fn test_non_json_content_type_is_rejected() {
    let route = post("/users", test_post_handler);
    let test_data = TestRequest {
        name: "Test User".to_string(),
        value: 21,
    };

    let req = Request::new(Method::POST, "/users".to_string())
        .with_header("Content-Type", "text/plain")
        .with_body(serde_json::to_vec(&test_data).unwrap());

    let result = route.handle(req).await.unwrap();
    assert_eq!(result.status, 400);
}

#[tokio::test]
async fn test_async_get_handler_execution() {
    let route = async_get("/test", test_async_get_handler);
    let req = Request::new(Method::GET, "/test".to_string());

    let result = route.handle(req).await.unwrap();
    assert_eq!(result.status, 200);

    let body_str = String::from_utf8(result.body.unwrap().to_vec()).unwrap();
    let response: TestResponse = serde_json::from_str(&body_str).unwrap();

    assert_eq!(response.message, "Hello from async GET");
    assert_eq!(response.value, 100);
}

#[tokio::test]
async fn test_async_post_handler_execution() {
    let route = async_post("/users", test_async_post_handler);
    let req = Request::new(Method::POST, "/users".to_string())
        .with_header("Content-Type", "application/json")
        .with_body(
            serde_json::to_vec(&TestRequest {
                name: "Test User".to_string(),
                value: 21,
            })
            .unwrap(),
        );

    let result = route.handle(req).await.unwrap();
    assert_eq!(result.status, 200);

    let body_str = String::from_utf8(result.body.unwrap().to_vec()).unwrap();
    let response: TestResponse = serde_json::from_str(&body_str).unwrap();

    assert_eq!(response.message, "Hello async, Test User");
    assert_eq!(response.value, 63);
}

#[tokio::test]
async fn test_invalid_route_templates_are_rejected() {
    assert!(super::core::try_route(
        Method::GET,
        "^/items/\\d+$",
        super::core::sync_handler(test_get_handler),
    )
    .is_err());
    assert!(super::core::try_route(
        Method::GET,
        "/users/{bad-name}",
        super::core::sync_handler(test_get_handler),
    )
    .is_err());
}

#[tokio::test]
async fn test_route_can_return_response_directly() {
    fn custom_header_handler(_req: Request) -> Result<Response, Error> {
        Ok(Response::ok()
            .with_header("X-Custom-Header", "CustomValue")
            .with_header("X-API-Version", "1.0")
            .with_body("ok".as_bytes().to_vec()))
    }

    let route = get("/test", custom_header_handler);
    let response = route
        .handle(Request::new(Method::GET, "/test".to_string()))
        .await
        .unwrap();

    assert_eq!(response.headers.get("X-Custom-Header"), Some("CustomValue"));
    assert_eq!(response.headers.get("X-API-Version"), Some("1.0"));
}

#[tokio::test]
async fn test_path_extractor() {
    let req = Request::new(Method::GET, "/users/123".to_string()).with_path_param("id", "123");
    let parts = RequestParts::from(&req);

    let path = Path::<TestPathParams>::from_request_parts(&parts)
        .await
        .unwrap();
    assert_eq!(path.0, TestPathParams { id: 123 });
}

#[tokio::test]
async fn test_path_extractor_rejection() {
    let req = Request::new(Method::GET, "/users/abc".to_string()).with_path_param("id", "abc");
    let parts = RequestParts::from(&req);

    assert!(Path::<TestPathParams>::from_request_parts(&parts)
        .await
        .is_err());
}

#[tokio::test]
async fn test_path_extractor_preserves_plus_sign() {
    let req = Request::new(Method::GET, "/users/alice+bob".to_string())
        .with_path_param("id", "alice+bob");
    let parts = RequestParts::from(&req);

    let path = Path::<StringPathParams>::from_request_parts(&parts)
        .await
        .unwrap();
    assert_eq!(
        path.0,
        StringPathParams {
            id: "alice+bob".to_string(),
        }
    );
}

#[tokio::test]
async fn test_query_extractor() {
    let req = Request::new(Method::GET, "/search".to_string())
        .with_query_param("tag", "a")
        .with_query_param("tag", "b")
        .with_query_param("page", "2");
    let parts = RequestParts::from(&req);

    let query = Query::<TestQueryParams>::from_request_parts(&parts)
        .await
        .unwrap();
    assert_eq!(
        query.0,
        TestQueryParams {
            tag: vec!["a".to_string(), "b".to_string()],
            page: 2,
        }
    );
}

#[tokio::test]
async fn test_query_extractor_accepts_single_value_vec() {
    let req = Request::new(Method::GET, "/search".to_string()).with_query_param("tag", "a");
    let parts = RequestParts::from(&req);

    let query = Query::<SingleTagQueryParams>::from_request_parts(&parts)
        .await
        .unwrap();
    assert_eq!(
        query.0,
        SingleTagQueryParams {
            tag: vec!["a".to_string()],
        }
    );
}

#[tokio::test]
async fn test_query_extractor_preserves_string_values() {
    let req = Request::new(Method::GET, "/search".to_string())
        .with_query_param("code", "00123")
        .with_query_param("flag", "true");
    let parts = RequestParts::from(&req);

    let query = Query::<StringQueryParams>::from_request_parts(&parts)
        .await
        .unwrap();
    assert_eq!(
        query.0,
        StringQueryParams {
            code: "00123".to_string(),
            flag: "true".to_string(),
        }
    );
}

#[tokio::test]
async fn test_route_path_params_preserve_plus_sign() {
    let route = get("/users/{id}", test_get_handler);
    let params = route
        .match_path("/users/alice+bob")
        .expect("route must match");

    assert_eq!(params.get("id"), Some(&"alice+bob".to_string()));
}
