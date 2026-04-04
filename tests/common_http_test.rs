use runbridge::common::get_max_body_size;
use runbridge::common::http::{
    HeaderMap, Method, QueryMap, Request, Response, ResponseBuilder, StatusCode,
};
use runbridge::common::Cookie;
use runbridge::error::Error;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

#[test]
fn test_method_from_str() {
    assert_eq!(Method::from_str("GET"), Ok(Method::GET));
    assert_eq!(Method::from_str("get"), Ok(Method::GET));
    assert_eq!(Method::from_str("POST"), Ok(Method::POST));
    assert_eq!(Method::from_str("PUT"), Ok(Method::PUT));
    assert_eq!(Method::from_str("DELETE"), Ok(Method::DELETE));
    assert_eq!(Method::from_str("PATCH"), Ok(Method::PATCH));
    assert_eq!(Method::from_str("HEAD"), Ok(Method::HEAD));
    assert_eq!(Method::from_str("OPTIONS"), Ok(Method::OPTIONS));
    assert_eq!(Method::from_str("INVALID"), Err(()));
}

#[test]
fn test_query_map_multi_value() {
    let mut query = QueryMap::new();
    query.append("tag", "a");
    query.append("tag", "b");
    query.append("sort", "desc");

    assert_eq!(query.get("tag"), Some("a"));
    assert_eq!(query.get_all("tag"), vec!["a", "b"]);
    assert_eq!(query.get("sort"), Some("desc"));
}

#[test]
fn test_header_map_case_insensitive_multi_value() {
    let mut headers = HeaderMap::new();
    headers.append("Content-Type", "application/json");
    headers.append("x-test", "a");
    headers.append("X-Test", "b");

    assert_eq!(headers.get("content-type"), Some("application/json"));
    assert_eq!(headers.get("Content-Type"), Some("application/json"));
    assert_eq!(headers.get_all("X-Test"), vec!["a", "b"]);
}

#[test]
fn test_request_builder() {
    let req = Request::new(Method::GET, "/test".to_string())
        .with_query_param("key1", "value1")
        .with_query_param("key1", "value2")
        .with_header("Content-Type", "application/json")
        .with_path_param("id", "123")
        .with_cookie(Cookie::new("session", "abc"))
        .with_body(b"test body".to_vec());

    assert_eq!(req.method, Method::GET);
    assert_eq!(req.path, "/test");
    assert_eq!(req.query.get_all("key1"), vec!["value1", "value2"]);
    assert_eq!(req.headers.get("content-type"), Some("application/json"));
    assert_eq!(req.path_params.get("id").map(String::as_str), Some("123"));
    assert_eq!(req.cookies[0].name, "session");
    assert_eq!(req.body.as_ref().unwrap().as_ref(), b"test body");
}

#[test]
fn test_response_builder() {
    let res = Response::ok()
        .with_header("Content-Type", "text/plain")
        .with_cookie(Cookie::new("session", "abc"))
        .with_body(b"Hello, world!".to_vec());

    assert_eq!(res.status, 200);
    assert_eq!(res.headers.get("content-type"), Some("text/plain"));
    assert_eq!(res.cookies.len(), 1);
    assert_eq!(res.body.as_ref().unwrap().as_ref(), b"Hello, world!");
}

#[test]
fn test_header_value_validation_rejects_crlf() {
    let req = Request::new(Method::GET, "/".to_string())
        .with_header("X-Test", "ok-value")
        .with_header("X-Bad", "bad\r\ninjected: 1");
    assert_eq!(req.headers.get("x-test"), Some("ok-value"));
    assert!(req.headers.get("x-bad").is_none());

    let res = Response::ok()
        .with_header("X-Good", "value")
        .with_header("X-Evil", "evil\nvalue");
    assert_eq!(res.headers.get("X-Good"), Some("value"));
    assert!(res.headers.get("X-Evil").is_none());

    let built = ResponseBuilder::new(200)
        .header("A", "v1")
        .header("B", "bad\rvalue")
        .build();
    assert_eq!(built.headers.get("A"), Some("v1"));
    assert!(built.headers.get("B").is_none());
}

#[test]
fn test_from_error_payload_too_large() {
    let err = Error::PayloadTooLarge("exceeds".to_string());
    let res = Response::from_error(&err);
    assert_eq!(res.status, 413);
    let body = String::from_utf8(res.body.unwrap().to_vec()).unwrap();
    assert_eq!(body, "Payload Too Large");
}

#[test]
fn test_from_error_internal_server_error_hides_details() {
    let err = Error::InternalServerError("db password=secret".to_string());
    let res = Response::from_error(&err);
    assert_eq!(res.status, 500);
    let body = String::from_utf8(res.body.unwrap().to_vec()).unwrap();
    assert_eq!(body, "Internal Server Error");
    assert!(!body.contains("secret"));
}

#[test]
fn test_from_error_route_not_found_hides_details() {
    let err = Error::RouteNotFound("GET /admin/internal".to_string());
    let res = Response::from_error(&err);
    assert_eq!(res.status, 404);
    let body = String::from_utf8(res.body.unwrap().to_vec()).unwrap();
    assert_eq!(body, "Not Found");
    assert!(!body.contains("/admin/internal"));
}

#[test]
fn test_from_error_resource_not_found_hides_details() {
    let err = Error::ResourceNotFound("user id=123".to_string());
    let res = Response::from_error(&err);
    assert_eq!(res.status, 404);
    let body = String::from_utf8(res.body.unwrap().to_vec()).unwrap();
    assert_eq!(body, "Not Found");
    assert!(!body.contains("123"));
}

#[test]
fn test_from_error_conflict() {
    let err = Error::Conflict("duplicate username".to_string());
    let res = Response::from_error(&err);
    assert_eq!(res.status, 409);
    let body = String::from_utf8(res.body.unwrap().to_vec()).unwrap();
    assert_eq!(body, "Conflict");
}

#[test]
fn test_from_error_unsupported_media_type() {
    let err = Error::UnsupportedMediaType("text/plain".to_string());
    let res = Response::from_error(&err);
    assert_eq!(res.status, 415);
    let body = String::from_utf8(res.body.unwrap().to_vec()).unwrap();
    assert_eq!(body, "Unsupported Media Type");
}

#[test]
fn test_from_error_validation_error() {
    let err = Error::ValidationError("name is required".to_string());
    let res = Response::from_error(&err);
    assert_eq!(res.status, 422);
    let body = String::from_utf8(res.body.unwrap().to_vec()).unwrap();
    assert_eq!(body, "Unprocessable Entity");
}

#[test]
fn test_from_error_too_many_requests() {
    let err = Error::TooManyRequests("rate limited".to_string());
    let res = Response::from_error(&err);
    assert_eq!(res.status, 429);
    let body = String::from_utf8(res.body.unwrap().to_vec()).unwrap();
    assert_eq!(body, "Too Many Requests");
}

#[test]
fn test_from_error_not_implemented() {
    let err = Error::NotImplemented("feature pending".to_string());
    let res = Response::from_error(&err);
    assert_eq!(res.status, 501);
    let body = String::from_utf8(res.body.unwrap().to_vec()).unwrap();
    assert_eq!(body, "Not Implemented");
}

#[test]
fn test_from_error_external_service_error() {
    let err = Error::ExternalServiceError("upstream returned garbage".to_string());
    let res = Response::from_error(&err);
    assert_eq!(res.status, 502);
    let body = String::from_utf8(res.body.unwrap().to_vec()).unwrap();
    assert_eq!(body, "Bad Gateway");
}

#[test]
fn test_from_error_service_unavailable() {
    let err = Error::ServiceUnavailable("db pool exhausted".to_string());
    let res = Response::from_error(&err);
    assert_eq!(res.status, 503);
    let body = String::from_utf8(res.body.unwrap().to_vec()).unwrap();
    assert_eq!(body, "Service Unavailable");
}

#[test]
fn test_from_error_gateway_timeout() {
    let err = Error::GatewayTimeout("upstream timeout".to_string());
    let res = Response::from_error(&err);
    assert_eq!(res.status, 504);
    let body = String::from_utf8(res.body.unwrap().to_vec()).unwrap();
    assert_eq!(body, "Gateway Timeout");
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
    assert_eq!(res.headers.get("Content-Type"), Some("application/json"));

    let body_str = String::from_utf8(res.body.unwrap().to_vec()).unwrap();
    let decoded: TestData = serde_json::from_str(&body_str).unwrap();
    assert_eq!(decoded, test_data);
}

#[test]
fn test_request_json() {
    let test_data = TestData {
        name: "test".to_string(),
        value: 42,
    };

    let json_bytes = serde_json::to_vec(&test_data).unwrap();
    let req = Request::new(Method::POST, "/test".to_string())
        .with_header("Content-Type", "application/json")
        .with_body(json_bytes);

    let parsed: TestData = req.json().unwrap();
    assert_eq!(parsed, test_data);
}

#[test]
fn test_status_code() {
    assert_eq!(StatusCode::Ok.as_u16(), 200);
    assert_eq!(StatusCode::Created.as_u16(), 201);
    assert_eq!(StatusCode::BadRequest.as_u16(), 400);
    assert_eq!(StatusCode::Unauthorized.as_u16(), 401);
    assert_eq!(StatusCode::Conflict.as_u16(), 409);
    assert_eq!(StatusCode::PayloadTooLarge.as_u16(), 413);
    assert_eq!(StatusCode::UnsupportedMediaType.as_u16(), 415);
    assert_eq!(StatusCode::UnprocessableEntity.as_u16(), 422);
    assert_eq!(StatusCode::TooManyRequests.as_u16(), 429);
    assert_eq!(StatusCode::InternalServerError.as_u16(), 500);
    assert_eq!(StatusCode::NotImplemented.as_u16(), 501);
    assert_eq!(StatusCode::BadGateway.as_u16(), 502);
    assert_eq!(StatusCode::ServiceUnavailable.as_u16(), 503);
    assert_eq!(StatusCode::GatewayTimeout.as_u16(), 504);

    assert_eq!(StatusCode::Ok.reason_phrase(), "OK");
    assert_eq!(StatusCode::Created.reason_phrase(), "Created");
    assert_eq!(StatusCode::BadRequest.reason_phrase(), "Bad Request");
    assert_eq!(StatusCode::Unauthorized.reason_phrase(), "Unauthorized");
    assert_eq!(StatusCode::Conflict.reason_phrase(), "Conflict");
    assert_eq!(
        StatusCode::PayloadTooLarge.reason_phrase(),
        "Payload Too Large"
    );
    assert_eq!(
        StatusCode::UnsupportedMediaType.reason_phrase(),
        "Unsupported Media Type"
    );
    assert_eq!(
        StatusCode::UnprocessableEntity.reason_phrase(),
        "Unprocessable Entity"
    );
    assert_eq!(
        StatusCode::TooManyRequests.reason_phrase(),
        "Too Many Requests"
    );
    assert_eq!(
        StatusCode::InternalServerError.reason_phrase(),
        "Internal Server Error"
    );
    assert_eq!(
        StatusCode::NotImplemented.reason_phrase(),
        "Not Implemented"
    );
    assert_eq!(StatusCode::BadGateway.reason_phrase(), "Bad Gateway");
    assert_eq!(
        StatusCode::ServiceUnavailable.reason_phrase(),
        "Service Unavailable"
    );
    assert_eq!(
        StatusCode::GatewayTimeout.reason_phrase(),
        "Gateway Timeout"
    );

    assert!(StatusCode::Ok.is_success());
    assert!(!StatusCode::BadRequest.is_success());
    assert!(StatusCode::BadRequest.is_client_error());
    assert!(StatusCode::Conflict.is_client_error());
    assert!(StatusCode::PayloadTooLarge.is_client_error());
    assert!(StatusCode::UnsupportedMediaType.is_client_error());
    assert!(StatusCode::UnprocessableEntity.is_client_error());
    assert!(StatusCode::TooManyRequests.is_client_error());
    assert!(StatusCode::InternalServerError.is_server_error());
    assert!(StatusCode::NotImplemented.is_server_error());
    assert!(StatusCode::BadGateway.is_server_error());
    assert!(StatusCode::ServiceUnavailable.is_server_error());
    assert!(StatusCode::GatewayTimeout.is_server_error());
}

#[test]
fn test_status_code_from_u16() {
    assert_eq!(StatusCode::from_u16(409), Some(StatusCode::Conflict));
    assert_eq!(StatusCode::from_u16(413), Some(StatusCode::PayloadTooLarge));
    assert_eq!(
        StatusCode::from_u16(415),
        Some(StatusCode::UnsupportedMediaType)
    );
    assert_eq!(
        StatusCode::from_u16(422),
        Some(StatusCode::UnprocessableEntity)
    );
    assert_eq!(StatusCode::from_u16(429), Some(StatusCode::TooManyRequests));
    assert_eq!(StatusCode::from_u16(501), Some(StatusCode::NotImplemented));
    assert_eq!(StatusCode::from_u16(502), Some(StatusCode::BadGateway));
    assert_eq!(
        StatusCode::from_u16(503),
        Some(StatusCode::ServiceUnavailable)
    );
    assert_eq!(StatusCode::from_u16(504), Some(StatusCode::GatewayTimeout));
    assert_eq!(StatusCode::from_u16(999), None);
}

#[test]
fn test_response_builder_methods() {
    let response = ResponseBuilder::with_status(StatusCode::Created)
        .security_headers()
        .header("X-Test", "test-value")
        .cookie(Cookie::new("session", "abc"))
        .text("Hello")
        .build();

    assert_eq!(response.status, StatusCode::Created.as_u16());
    assert!(response.headers.contains_key("X-Content-Type-Options"));
    assert_eq!(response.headers.get("X-Test"), Some("test-value"));
    assert_eq!(
        response.headers.get("Content-Type"),
        Some("text/plain; charset=utf-8")
    );
    assert_eq!(response.cookies.len(), 1);
    assert_eq!(
        String::from_utf8(response.body.unwrap().to_vec()).unwrap(),
        "Hello"
    );
}

#[test]
fn test_response_builder_with_json() {
    #[derive(Serialize)]
    struct TestJson {
        message: String,
    }

    let response = ResponseBuilder::new(200)
        .json(&TestJson {
            message: "Hi".to_string(),
        })
        .unwrap()
        .build();

    assert_eq!(response.status, 200);
    assert_eq!(
        response.headers.get("Content-Type"),
        Some("application/json")
    );
}

#[test]
fn test_request_clone_without_context() {
    let mut req = Request::new(Method::POST, "/test".to_string())
        .with_query_param("key1", "value1")
        .with_header("Content-Type", "application/json")
        .with_body(b"test body".to_vec());

    req.context_mut().set("user_id", 123u32);
    req.context_mut().set("session", "abc123".to_string());

    let cloned = req.clone_without_context();

    assert_eq!(cloned.method, req.method);
    assert_eq!(cloned.path, req.path);
    assert_eq!(cloned.query, req.query);
    assert_eq!(cloned.headers, req.headers);
    assert_eq!(cloned.body, req.body);
    assert!(cloned.context().is_empty());
    assert!(!cloned.context().contains_key("user_id"));
    assert!(req.context().contains_key("user_id"));
}

#[test]
fn test_decompress_gzip_body_success() {
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use std::io::Write;

    let original_data = r#"{"message": "Hello, World!", "compressed": true}"#;
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(original_data.as_bytes()).unwrap();
    let compressed_data = encoder.finish().unwrap();

    let mut request = Request::new(Method::POST, "/test".to_string())
        .with_header("Content-Type", "application/json")
        .with_header("Content-Encoding", "gzip")
        .with_body(compressed_data);

    assert_eq!(request.headers.get("content-encoding"), Some("gzip"));
    request.decompress_gzip_body().unwrap();
    assert_eq!(
        String::from_utf8(request.body.unwrap().to_vec()).unwrap(),
        original_data
    );
    assert!(request.headers.get("content-encoding").is_none());
}

#[test]
fn test_decompress_gzip_body_no_encoding_header() {
    let original_data = "This is not compressed";
    let mut request = Request::new(Method::POST, "/test".to_string())
        .with_header("Content-Type", "text/plain")
        .with_body(original_data.as_bytes().to_vec());

    request.decompress_gzip_body().unwrap();
    assert_eq!(
        String::from_utf8(request.body.unwrap().to_vec()).unwrap(),
        original_data
    );
}

#[test]
fn test_gzip_decompression_uses_same_body_size_limit() {
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use std::io::Write;

    let max = get_max_body_size();
    let original_data = vec![b'a'; max + 1];
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(&original_data).unwrap();
    let compressed_data = encoder.finish().unwrap();

    let mut request = Request::new(Method::POST, "/test".to_string())
        .with_header("Content-Type", "text/plain")
        .with_header("Content-Encoding", "gzip")
        .with_body(compressed_data);

    let result = request.decompress_gzip_body();
    assert!(matches!(result, Err(Error::PayloadTooLarge(_))));
}
