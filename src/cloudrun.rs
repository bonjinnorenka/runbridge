//! Google Cloud Run向けの実装

use actix_web::http::header::HeaderMap as ActixHeaderMap;
use actix_web::web::Bytes;
use actix_web::{web, App, HttpRequest, HttpResponse, HttpServer};
use log::{info, warn};
use std::sync::Arc;

use crate::common::{
    get_max_body_size, parse_cookie_header, parse_query_string, HeaderMap, Method, Request,
    Response,
};
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

async fn convert_request(req: &HttpRequest, path: String, body: Option<Bytes>) -> Request {
    let method = Method::from_str(req.method().as_str()).unwrap_or(Method::GET);
    let headers = convert_headers(req.headers());
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

    if let Err(err) = request.decompress_gzip_body() {
        warn!("Failed to decompress gzip body in Cloud Run: {}", err);
    }

    request
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
            warn!("Request body too large: {} bytes (limit {})", body.len(), max);
            return HttpResponse::PayloadTooLarge().finish();
        }
    }

    let request = convert_request(&req, path, body).await;
    let response = app.handle_request(request).await;
    convert_to_http_response(response)
}

pub async fn run_cloud_run(app: RunBridge, host: &str, port: u16) -> std::io::Result<()> {
    info!("Starting HTTP server on {}:{}", host, port);

    let app_data = Arc::new(app);
    let max_body = get_max_body_size();

    HttpServer::new(move || {
        let app_data = web::Data::new(app_data.clone());

        App::new()
            .app_data(app_data.clone())
            .app_data(web::PayloadConfig::new(max_body))
            .route(
                "/{path:.*}",
                web::get().to(|req, app: web::Data<Arc<RunBridge>>| handle_request(req, None, app)),
            )
            .route(
                "/{path:.*}",
                web::post().to(|req, body: Option<Bytes>, app: web::Data<Arc<RunBridge>>| {
                    handle_request(req, body, app)
                }),
            )
            .route(
                "/{path:.*}",
                web::put().to(|req, body: Option<Bytes>, app: web::Data<Arc<RunBridge>>| {
                    handle_request(req, body, app)
                }),
            )
            .route(
                "/{path:.*}",
                web::delete()
                    .to(|req, app: web::Data<Arc<RunBridge>>| handle_request(req, None, app)),
            )
            .route(
                "/{path:.*}",
                web::patch().to(|req, body: Option<Bytes>, app: web::Data<Arc<RunBridge>>| {
                    handle_request(req, body, app)
                }),
            )
            .route(
                "/{path:.*}",
                web::head()
                    .to(|req, app: web::Data<Arc<RunBridge>>| handle_request(req, None, app)),
            )
            .route(
                "/{path:.*}",
                web::method(actix_web::http::Method::OPTIONS)
                    .to(|req, app: web::Data<Arc<RunBridge>>| handle_request(req, None, app)),
            )
    })
    .bind((host, port))?
    .run()
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::Cookie;
    use actix_web::test::TestRequest;

    #[actix_rt::test]
    async fn convert_request_extracts_query_headers_and_cookies() {
        let req = TestRequest::get()
            .uri("/items/123?tag=a&tag=b")
            .insert_header(("X-Test", "one"))
            .insert_header(("Cookie", "session=abc; theme=dark"))
            .to_http_request();

        let request = convert_request(&req, "/items/123".to_string(), None).await;
        assert_eq!(request.query.get_all("tag"), vec!["a", "b"]);
        assert_eq!(request.headers.get("x-test"), Some("one"));
        assert_eq!(request.cookies.len(), 2);
        assert_eq!(request.cookies[0].name, "session");
    }

    #[test]
    fn convert_to_http_response_preserves_multiple_cookies() {
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
        assert!(set_cookie_values.iter().any(|value| value.starts_with("session=abc")));
        assert!(set_cookie_values.iter().any(|value| value.starts_with("theme=dark")));
    }
}
