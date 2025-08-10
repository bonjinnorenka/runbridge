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

    Ok(
        Response::ok()
            .with_header("X-Custom-Header", "CustomValue")
            .with_header("X-API-Version", "1.0")
            .json(&response_data)?,
    )
}

// 非同期でカスタムヘッダーを返すハンドラー
async fn test_async_custom_header_handler(_req: Request) -> Result<Response, Error> {
    let response_data = TestResponse {
        message: "Async response with custom header".to_string(),
        value: 456,
    };

    Ok(
        Response::ok()
            .with_header("X-Custom-Header", "AsyncValue")
            .with_header("X-API-Version", "2.0")
            .json(&response_data)?,
    )
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
        Err(Error::InvalidRequestBody(_)) => {}
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
async fn test_options_handler_execution() {
    let handler = get("/options", test_options_handler);
    let req = Request::new(Method::GET, "/options".to_string());

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
    let body = serde_json::to_vec(&TestRequest {
        name: "john".into(),
        value: 7,
    })
    .unwrap();
    let req = Request::new(Method::POST, "/plus-json".to_string())
        .with_header("Content-Type", "application/ld+json; charset=utf-8")
        .with_body(body);

    let res = handler
        .handle(req)
        .await
        .expect("handler should accept +json");
    assert_eq!(res.status, 200);
}

#[tokio::test]
async fn test_content_type_accept_json_seq() {
    // application/json-seq も許容リストに含める
    let handler = post("/json-seq", test_post_handler);

    let body = serde_json::to_vec(&TestRequest {
        name: "seq".into(),
        value: 3,
    })
    .unwrap();
    let req = Request::new(Method::POST, "/json-seq".to_string())
        .with_header("Content-Type", "application/json-seq")
        .with_body(body);

    let res = handler
        .handle(req)
        .await
        .expect("handler should accept application/json-seq");
    assert_eq!(res.status, 200);
}

#[tokio::test]
async fn test_content_type_reject_non_json() {
    // POSTハンドラー
    let handler = post("/reject", test_post_handler);

    // JSONボディだが `text/plain` は非許容
    let body = serde_json::to_vec(&TestRequest {
        name: "doe".into(),
        value: 1,
    })
    .unwrap();
    let req = Request::new(Method::POST, "/reject".to_string())
        .with_header("Content-Type", "text/plain")
        .with_body(body);

    let err = handler
        .handle(req)
        .await
        .expect_err("handler should reject non-json content-type");
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
    let body = serde_json::to_vec(&TestRequest {
        name: "case".into(),
        value: 2,
    })
    .unwrap();
    let req = Request::new(Method::POST, "/case-insensitive".to_string())
        .with_header("content-type", "application/json; charset=utf-8")
        .with_body(body);

    let res = handler
        .handle(req)
        .await
        .expect("header lookup should be case-insensitive");
    assert_eq!(res.status, 200);
}

#[tokio::test]
async fn test_empty_body_skips_validation_for_get() {
    // GETハンドラー（T=()）: 空ボディ（長さ0）ならパースも検証もスキップ
    let handler = get("/empty", test_get_handler);
    let req = Request::new(Method::GET, "/empty".to_string())
        .with_header("Content-Type", "text/plain") // 本来非対応だが空ボディなら影響しない
        .with_body(Vec::new()); // 空ボディ

    let res = handler
        .handle(req)
        .await
        .expect("empty body should be ignored for GET");
    assert_eq!(res.status, 200);
}

#[tokio::test]
async fn test_empty_body_treated_as_missing_for_post() {
    // POSTハンドラー: 空ボディ（長さ0）はMissing request bodyとしてエラー
    let handler = post("/empty-post", test_post_handler);
    let req = Request::new(Method::POST, "/empty-post".to_string())
        .with_header("Content-Type", "application/json")
        .with_body(Vec::new()); // 空ボディ

    let err = handler
        .handle(req)
        .await
        .expect_err("empty body should be treated as missing for POST");
    match err {
        Error::InvalidRequestBody(msg) => assert!(msg.contains("Missing request body")),
        e => panic!("unexpected error variant: {:?}", e),
    }
}
