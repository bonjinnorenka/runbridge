//! Google Cloud Run向けの実装

use actix_web::http::header::HeaderMap as ActixHeaderMap;
use actix_web::middleware::Compress;
use actix_web::web::Bytes;
use actix_web::{web, App, HttpRequest, HttpResponse, HttpServer};
use log::{info, warn};
use std::str::FromStr;
use std::sync::Arc;

use crate::common::{
    get_max_body_size, parse_cookie_header, parse_query_string, HeaderMap, Method, Request,
    Response,
};
use crate::error::Error;
use crate::RunBridge;

fn convert_headers(headers: &ActixHeaderMap) -> HeaderMap {
    let mut result = HeaderMap::new();

    for key in headers.keys() {
        for value in headers.get_all(key) {
            if let Ok(value_str) = value.to_str() {
                result.append(key.as_str().to_string(), value_str.to_string());
            }
        }
    }

    result
}

async fn convert_request(
    req: &HttpRequest,
    path: String,
    body: Option<Bytes>,
) -> Result<Request, Error> {
    let method = Method::from_str(req.method().as_str()).unwrap_or(Method::GET);
    let mut headers = convert_headers(req.headers());
    if headers
        .get("content-encoding")
        .is_some_and(|encoding| encoding.eq_ignore_ascii_case("gzip"))
    {
        headers.remove("content-encoding");
    }
    let cookies = headers
        .get("cookie")
        .map(parse_cookie_header)
        .unwrap_or_default();
    let query = parse_query_string(req.query_string());
    let body = body.map(|body| body.to_vec().into());

    let mut request = Request::new(method, path);
    request.query = query;
    request.headers = headers;
    request.cookies = cookies;
    request.body = body;

    Ok(request)
}

fn convert_to_http_response(response: Response) -> HttpResponse {
    let mut builder = HttpResponse::build(
        actix_web::http::StatusCode::from_u16(response.status)
            .unwrap_or(actix_web::http::StatusCode::INTERNAL_SERVER_ERROR),
    );

    for (key, value) in &response.headers {
        builder.append_header((key, value));
    }

    for cookie in response.cookies {
        builder.append_header(("Set-Cookie", cookie.to_header_value()));
    }

    if let Some(body) = response.body {
        builder.body(body)
    } else {
        builder.finish()
    }
}

async fn handle_request(
    req: HttpRequest,
    body: Option<Bytes>,
    app: web::Data<Arc<RunBridge>>,
) -> HttpResponse {
    let path = req.uri().path().to_string();
    info!("Received request: {} {}", req.method(), path);

    if let Some(ref body) = body {
        let max = get_max_body_size();
        if body.len() > max {
            warn!(
                "Request body too large: {} bytes (limit {})",
                body.len(),
                max
            );
            return HttpResponse::PayloadTooLarge().finish();
        }
    }

    let request = match convert_request(&req, path, body).await {
        Ok(request) => request,
        Err(err) => {
            warn!("Failed to convert Cloud Run request: {}", err);
            return convert_to_http_response(Response::from_error(&err));
        }
    };
    let response = app.handle_request(request).await;
    convert_to_http_response(response)
}

async fn handle_request_without_body(
    req: HttpRequest,
    app: web::Data<Arc<RunBridge>>,
) -> HttpResponse {
    handle_request(req, None, app).await
}

async fn handle_request_with_body(
    req: HttpRequest,
    body: Bytes,
    app: web::Data<Arc<RunBridge>>,
) -> HttpResponse {
    handle_request(req, Some(body), app).await
}

fn configure_cloud_run_routes(cfg: &mut web::ServiceConfig) {
    cfg.route("/{path:.*}", web::get().to(handle_request_without_body))
        .route("/{path:.*}", web::post().to(handle_request_with_body))
        .route("/{path:.*}", web::put().to(handle_request_with_body))
        .route("/{path:.*}", web::delete().to(handle_request_without_body))
        .route("/{path:.*}", web::patch().to(handle_request_with_body))
        .route("/{path:.*}", web::head().to(handle_request_without_body))
        .route(
            "/{path:.*}",
            web::method(actix_web::http::Method::OPTIONS).to(handle_request_without_body),
        );
}

pub async fn run_cloud_run(app: RunBridge, host: &str, port: u16) -> std::io::Result<()> {
    info!("Starting HTTP server on {}:{}", host, port);

    let app_data = Arc::new(app);
    let max_body = get_max_body_size();

    HttpServer::new(move || {
        let app_data = web::Data::new(app_data.clone());

        App::new()
            .wrap(Compress::default())
            .app_data(app_data.clone())
            .app_data(web::PayloadConfig::new(max_body))
            .configure(configure_cloud_run_routes)
    })
    .bind((host, port))?
    .run()
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::Cookie;
    use crate::common::StatusCode;
    use crate::handler;
    use actix_web::test;
    use actix_web::test::TestRequest;
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use serde_json::Value;
    use std::io::Read;
    use std::io::Write;
    use std::sync::Mutex;

    #[actix_rt::test]
    async fn convert_request_extracts_query_headers_and_cookies() {
        let req = TestRequest::get()
            .uri("/items/123?tag=a&tag=b")
            .insert_header(("X-Test", "one"))
            .insert_header(("Cookie", "session=abc; theme=dark"))
            .to_http_request();

        let request = convert_request(&req, "/items/123".to_string(), None)
            .await
            .unwrap();
        assert_eq!(request.query.get_all("tag"), vec!["a", "b"]);
        assert_eq!(request.headers.get("x-test"), Some("one"));
        assert_eq!(request.cookies.len(), 2);
        assert_eq!(request.cookies[0].name, "session");
    }

    #[actix_rt::test]
    async fn convert_request_removes_gzip_header_without_redecoding_body() {
        let req = TestRequest::post()
            .uri("/upload")
            .insert_header(("Content-Encoding", "gzip"))
            .insert_header(("Content-Type", "application/json"))
            .to_http_request();

        let request = convert_request(
            &req,
            "/upload".to_string(),
            Some(Bytes::from_static(br#"{"message":"ok"}"#)),
        )
        .await
        .expect("cloud run request conversion must not re-decode gzip body");

        assert_eq!(
            request.body,
            Some(Bytes::from_static(br#"{"message":"ok"}"#))
        );
        assert!(request.headers.get("content-encoding").is_none());
    }

    #[actix_rt::test]
    async fn convert_to_http_response_preserves_binary_fixed_file_body() {
        let response = Response::ok()
            .with_header("Content-Type", "image/png")
            .with_body(vec![0x89, 0x50, 0x4e, 0x47]);

        let converted = convert_to_http_response(response);

        assert_eq!(converted.status(), actix_web::http::StatusCode::OK);
        assert_eq!(
            converted
                .headers()
                .get("Content-Type")
                .and_then(|value| value.to_str().ok()),
            Some("image/png")
        );
        let body = actix_web::body::to_bytes(converted.into_body())
            .await
            .expect("response body must be readable");
        assert_eq!(body.as_ref(), &[0x89, 0x50, 0x4e, 0x47]);
    }

    #[actix_rt::test]
    async fn cloud_run_binary_response_works_with_compression_middleware() {
        let app = Arc::new(
            RunBridge::builder()
                .handler(handler::get("/image", |_req| {
                    Ok(Response::ok()
                        .with_header("Content-Type", "image/png")
                        .with_body(vec![0x89, 0x50, 0x4e, 0x47]))
                }))
                .build(),
        );

        let service = test::init_service(
            App::new()
                .wrap(Compress::default())
                .app_data(web::Data::new(app))
                .app_data(web::PayloadConfig::new(get_max_body_size()))
                .configure(configure_cloud_run_routes),
        )
        .await;

        let response = test::call_service(
            &service,
            TestRequest::get()
                .uri("/image")
                .insert_header(("Accept-Encoding", "gzip"))
                .to_request(),
        )
        .await;

        assert_eq!(response.status(), actix_web::http::StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get("Content-Type")
                .and_then(|value| value.to_str().ok()),
            Some("image/png")
        );

        let body = actix_web::body::to_bytes(response.into_body())
            .await
            .expect("response body must be readable");

        assert_eq!(body.as_ref(), &[0x89, 0x50, 0x4e, 0x47]);
    }

    #[actix_rt::test]
    async fn cloud_run_accepts_gzip_json_and_exposes_decompressed_request() {
        let seen_request = Arc::new(Mutex::new(None));
        let seen_request_for_handler = Arc::clone(&seen_request);
        let app = Arc::new(
            RunBridge::builder()
                .handler(handler::post("/upload", move |req, body: Value| {
                    *seen_request_for_handler.lock().unwrap() = Some(req);
                    Ok(serde_json::json!({
                        "message": body["message"].as_str().unwrap_or_default(),
                    }))
                }))
                .build(),
        );
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder
            .write_all(br#"{"message":"hello gzip"}"#)
            .expect("gzip payload write must succeed");
        let compressed = encoder.finish().expect("gzip payload must finalize");

        let service = test::init_service(
            App::new()
                .wrap(Compress::default())
                .app_data(web::Data::new(app))
                .app_data(web::PayloadConfig::new(get_max_body_size()))
                .configure(configure_cloud_run_routes),
        )
        .await;

        let response = test::call_service(
            &service,
            TestRequest::post()
                .uri("/upload")
                .insert_header(("Content-Type", "application/json"))
                .insert_header(("Content-Encoding", "gzip"))
                .set_payload(compressed)
                .to_request(),
        )
        .await;

        assert_eq!(response.status(), actix_web::http::StatusCode::OK);
        let response_body: Value = test::read_body_json(response).await;
        assert_eq!(response_body["message"], "hello gzip");

        let seen_request = seen_request
            .lock()
            .unwrap()
            .take()
            .expect("handler must receive a request");
        assert!(seen_request.headers.get("content-encoding").is_none());
        assert_eq!(
            seen_request.body,
            Some(Bytes::from_static(br#"{"message":"hello gzip"}"#))
        );
    }

    #[actix_rt::test]
    async fn cloud_run_returns_bad_request_for_invalid_gzip_body() {
        let app = Arc::new(
            RunBridge::builder()
                .handler(handler::post("/upload", |_req, _body: Value| {
                    Ok(Response::ok())
                }))
                .build(),
        );
        let service = test::init_service(
            App::new()
                .wrap(Compress::default())
                .app_data(web::Data::new(app))
                .app_data(web::PayloadConfig::new(get_max_body_size()))
                .configure(configure_cloud_run_routes),
        )
        .await;

        let response = test::call_service(
            &service,
            TestRequest::post()
                .uri("/upload")
                .insert_header(("Content-Type", "application/json"))
                .insert_header(("Content-Encoding", "gzip"))
                .set_payload("not-gzip")
                .to_request(),
        )
        .await;

        assert_eq!(response.status(), actix_web::http::StatusCode::BAD_REQUEST);
    }

    #[actix_rt::test]
    async fn cloud_run_returns_payload_too_large_for_oversized_gzip_body() {
        let app = Arc::new(
            RunBridge::builder()
                .handler(handler::post("/upload", |_req, _body: Value| {
                    Ok(Response::ok())
                }))
                .build(),
        );
        let service = test::init_service(
            App::new()
                .wrap(Compress::default())
                .app_data(web::Data::new(app))
                .app_data(web::PayloadConfig::new(8))
                .configure(configure_cloud_run_routes),
        )
        .await;

        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder
            .write_all(br#"{"message":"too large"}"#)
            .expect("gzip payload write must succeed");
        let compressed = encoder.finish().expect("gzip payload must finalize");

        let response = test::call_service(
            &service,
            TestRequest::post()
                .uri("/upload")
                .insert_header(("Content-Type", "application/json"))
                .insert_header(("Content-Encoding", "gzip"))
                .set_payload(compressed)
                .to_request(),
        )
        .await;

        assert_eq!(
            response.status(),
            actix_web::http::StatusCode::PAYLOAD_TOO_LARGE
        );
    }

    #[actix_rt::test]
    async fn convert_to_http_response_preserves_multiple_cookies() {
        let response = Response::ok()
            .append_header("X-Test", "one")
            .append_header("X-Test", "two")
            .with_cookie(Cookie::new("session", "abc"))
            .with_cookie(Cookie::new("theme", "dark"));

        let http_response = convert_to_http_response(response);
        let headers = http_response.headers();
        let set_cookie_values: Vec<_> = headers
            .get_all("Set-Cookie")
            .filter_map(|value| value.to_str().ok())
            .collect();

        assert_eq!(set_cookie_values.len(), 2);
        assert!(set_cookie_values
            .iter()
            .any(|value| value.starts_with("session=abc")));
        assert!(set_cookie_values
            .iter()
            .any(|value| value.starts_with("theme=dark")));
    }

    #[actix_rt::test]
    async fn cloud_run_compresses_response_with_accept_encoding_gzip() {
        let large_json = serde_json::json!({
            "data": "x".repeat(1000),
            "nested": {
                "items": vec!["item"; 100]
            }
        });

        let json_bytes = serde_json::to_vec(&large_json).unwrap();

        let app = Arc::new(
            RunBridge::builder()
                .handler(handler::get("/data", move |_req| {
                    Ok(Response::ok()
                        .with_header("Content-Type", "application/json")
                        .with_body(json_bytes.clone()))
                }))
                .build(),
        );

        let service = test::init_service(
            App::new()
                .wrap(Compress::default())
                .app_data(web::Data::new(app))
                .app_data(web::PayloadConfig::new(get_max_body_size()))
                .configure(configure_cloud_run_routes),
        )
        .await;

        let response = test::call_service(
            &service,
            TestRequest::get()
                .uri("/data")
                .insert_header(("Accept-Encoding", "gzip"))
                .to_request(),
        )
        .await;

        assert_eq!(response.status(), actix_web::http::StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get("Content-Encoding")
                .and_then(|value| value.to_str().ok()),
            Some("gzip")
        );
        assert_eq!(
            response
                .headers()
                .get("Content-Type")
                .and_then(|value| value.to_str().ok()),
            Some("application/json")
        );

        let body = actix_web::body::to_bytes(response.into_body())
            .await
            .expect("response body must be readable");

        let mut decoder = flate2::read::GzDecoder::new(&body[..]);
        let mut decompressed = Vec::new();
        decoder
            .read_to_end(&mut decompressed)
            .expect("decompression must succeed");

        let decompressed_json: Value =
            serde_json::from_slice(&decompressed).expect("decompressed body must be valid JSON");
        assert_eq!(decompressed_json, large_json);
    }

    #[actix_rt::test]
    async fn cloud_run_returns_uncompressed_with_accept_encoding_identity() {
        let large_json = serde_json::json!({
            "data": "x".repeat(1000),
            "nested": {
                "items": vec!["item"; 100]
            }
        });

        let json_bytes = serde_json::to_vec(&large_json).unwrap();

        let app = Arc::new(
            RunBridge::builder()
                .handler(handler::get("/data", move |_req| {
                    Ok(Response::ok()
                        .with_header("Content-Type", "application/json")
                        .with_body(json_bytes.clone()))
                }))
                .build(),
        );

        let service = test::init_service(
            App::new()
                .wrap(Compress::default())
                .app_data(web::Data::new(app))
                .app_data(web::PayloadConfig::new(get_max_body_size()))
                .configure(configure_cloud_run_routes),
        )
        .await;

        let response = test::call_service(
            &service,
            TestRequest::get()
                .uri("/data")
                .insert_header(("Accept-Encoding", "identity"))
                .to_request(),
        )
        .await;

        assert_eq!(response.status(), actix_web::http::StatusCode::OK);
        assert!(
            response.headers().get("Content-Encoding").is_none(),
            "Content-Encoding should not be set for identity"
        );
        assert_eq!(
            response
                .headers()
                .get("Content-Type")
                .and_then(|value| value.to_str().ok()),
            Some("application/json")
        );

        let body = actix_web::body::to_bytes(response.into_body())
            .await
            .expect("response body must be readable");

        let response_json: Value =
            serde_json::from_slice(&body).expect("response body must be valid JSON");
        assert_eq!(response_json, large_json);
    }

    #[actix_rt::test]
    async fn cloud_run_returns_normal_response_without_accept_encoding() {
        let large_json = serde_json::json!({
            "data": "x".repeat(1000),
            "nested": {
                "items": vec!["item"; 100]
            }
        });

        let json_bytes = serde_json::to_vec(&large_json).unwrap();

        let app = Arc::new(
            RunBridge::builder()
                .handler(handler::get("/data", move |_req| {
                    Ok(Response::ok()
                        .with_header("Content-Type", "application/json")
                        .with_body(json_bytes.clone()))
                }))
                .build(),
        );

        let service = test::init_service(
            App::new()
                .wrap(Compress::default())
                .app_data(web::Data::new(app))
                .app_data(web::PayloadConfig::new(get_max_body_size()))
                .configure(configure_cloud_run_routes),
        )
        .await;

        let response =
            test::call_service(&service, TestRequest::get().uri("/data").to_request()).await;

        assert_eq!(response.status(), actix_web::http::StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get("Content-Type")
                .and_then(|value| value.to_str().ok()),
            Some("application/json")
        );

        let body = actix_web::body::to_bytes(response.into_body())
            .await
            .expect("response body must be readable");

        let response_json: Value =
            serde_json::from_slice(&body).expect("response body must be valid JSON");
        assert_eq!(response_json, large_json);
    }

    #[actix_rt::test]
    async fn cloud_run_does_not_double_compress_already_encoded_response() {
        let large_json = serde_json::json!({
            "data": "x".repeat(1000),
            "nested": {
                "items": vec!["item"; 100]
            }
        });

        let json_bytes = serde_json::to_vec(&large_json).unwrap();

        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder
            .write_all(&json_bytes)
            .expect("gzip encoding must succeed");
        let compressed_body = encoder.finish().expect("gzip finalization must succeed");

        let app = Arc::new(
            RunBridge::builder()
                .handler(handler::get("/data", move |_req| {
                    Ok(Response::ok()
                        .with_header("Content-Type", "application/json")
                        .with_header("Content-Encoding", "gzip")
                        .with_body(compressed_body.clone()))
                }))
                .build(),
        );

        let service = test::init_service(
            App::new()
                .wrap(Compress::default())
                .app_data(web::Data::new(app))
                .app_data(web::PayloadConfig::new(get_max_body_size()))
                .configure(configure_cloud_run_routes),
        )
        .await;

        let response = test::call_service(
            &service,
            TestRequest::get()
                .uri("/data")
                .insert_header(("Accept-Encoding", "gzip"))
                .to_request(),
        )
        .await;

        assert_eq!(response.status(), actix_web::http::StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get("Content-Encoding")
                .and_then(|value| value.to_str().ok()),
            Some("gzip")
        );

        let body = actix_web::body::to_bytes(response.into_body())
            .await
            .expect("response body must be readable");

        let mut decoder = flate2::read::GzDecoder::new(&body[..]);
        let mut decompressed = Vec::new();
        decoder
            .read_to_end(&mut decompressed)
            .expect("decompression must succeed");

        let decompressed_json: Value =
            serde_json::from_slice(&decompressed).expect("decompressed body must be valid JSON");
        assert_eq!(decompressed_json, large_json);
    }

    #[actix_rt::test]
    async fn cloud_run_head_request_preserves_body_semantics() {
        let app = Arc::new(
            RunBridge::builder()
                .handler(handler::head("/data", |_req| {
                    Ok(Response::ok()
                        .with_header("Content-Type", "application/json")
                        .with_body(br#"{"message":"hello"}"#.to_vec()))
                }))
                .build(),
        );

        let service = test::init_service(
            App::new()
                .wrap(Compress::default())
                .app_data(web::Data::new(app))
                .app_data(web::PayloadConfig::new(get_max_body_size()))
                .configure(configure_cloud_run_routes),
        )
        .await;

        let response = test::call_service(
            &service,
            TestRequest::default()
                .method(actix_web::http::Method::HEAD)
                .uri("/data")
                .insert_header(("Accept-Encoding", "gzip"))
                .to_request(),
        )
        .await;

        assert_eq!(response.status(), actix_web::http::StatusCode::OK);

        let body = actix_web::body::to_bytes(response.into_body())
            .await
            .expect("response body must be readable");

        assert!(
            body.is_empty(),
            "HEAD request should return empty body even with compression"
        );
    }

    #[actix_rt::test]
    async fn cloud_run_204_no_content_remains_bodyless() {
        let app = Arc::new(
            RunBridge::builder()
                .handler(handler::post("/data", |_req, _body: Value| {
                    Ok(Response::with_status(StatusCode::NoContent))
                }))
                .build(),
        );

        let service = test::init_service(
            App::new()
                .wrap(Compress::default())
                .app_data(web::Data::new(app))
                .app_data(web::PayloadConfig::new(get_max_body_size()))
                .configure(configure_cloud_run_routes),
        )
        .await;

        let response = test::call_service(
            &service,
            TestRequest::post()
                .uri("/data")
                .insert_header(("Accept-Encoding", "gzip"))
                .set_json(&serde_json::json!({"test": "data"}))
                .to_request(),
        )
        .await;

        assert_eq!(response.status(), actix_web::http::StatusCode::NO_CONTENT);

        let body = actix_web::body::to_bytes(response.into_body())
            .await
            .expect("response body must be readable");

        assert!(
            body.is_empty(),
            "204 No Content should remain bodyless even with compression"
        );
    }
}
