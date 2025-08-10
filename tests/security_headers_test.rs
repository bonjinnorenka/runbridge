use runbridge::common::{Response, ResponseBuilder, StatusCode};

fn assert_default_security_headers(headers: &std::collections::HashMap<String, String>) {
    assert_eq!(headers.get("X-Content-Type-Options").map(|s| s.as_str()), Some("nosniff"));
    assert_eq!(headers.get("X-Frame-Options").map(|s| s.as_str()), Some("DENY"));
    assert_eq!(headers.get("X-XSS-Protection").map(|s| s.as_str()), Some("1; mode=block"));
    assert_eq!(headers.get("Referrer-Policy").map(|s| s.as_str()), Some("strict-origin-when-cross-origin"));
    assert_eq!(headers.get("Content-Security-Policy").map(|s| s.as_str()), Some("default-src 'self'"));
}

#[test]
fn response_has_default_security_headers() {
    let res = Response::ok();
    assert_default_security_headers(&res.headers);
}

#[test]
fn response_with_status_has_default_security_headers() {
    let res = Response::with_status(StatusCode::Created);
    assert_default_security_headers(&res.headers);
}

#[test]
fn response_allows_overrides() {
    let res = Response::ok()
        .with_header("X-Frame-Options", "SAMEORIGIN")
        .with_header("Content-Security-Policy", "default-src 'self' https:");

    assert_eq!(res.headers.get("X-Frame-Options").map(|s| s.as_str()), Some("SAMEORIGIN"));
    assert_eq!(res.headers.get("Content-Security-Policy").map(|s| s.as_str()), Some("default-src 'self' https:"));
}

#[test]
fn response_builder_has_default_security_headers() {
    let res = ResponseBuilder::new(200).build();
    assert_default_security_headers(&res.headers);
}

#[test]
fn response_builder_allows_overrides() {
    let res = ResponseBuilder::with_status(StatusCode::Ok)
        .header("X-Frame-Options", "SAMEORIGIN")
        .build();
    assert_eq!(res.headers.get("X-Frame-Options").map(|s| s.as_str()), Some("SAMEORIGIN"));
    // 他の既定は維持
    assert_eq!(res.headers.get("X-Content-Type-Options").map(|s| s.as_str()), Some("nosniff"));
}

