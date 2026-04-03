use runbridge::common::{HeaderMap, Response, ResponseBuilder, StatusCode};

fn assert_default_security_headers(headers: &HeaderMap) {
    assert_eq!(headers.get("X-Content-Type-Options"), Some("nosniff"));
    assert_eq!(headers.get("X-Frame-Options"), Some("DENY"));
    assert_eq!(headers.get("X-XSS-Protection"), Some("1; mode=block"));
    assert_eq!(
        headers.get("Referrer-Policy"),
        Some("strict-origin-when-cross-origin")
    );
    assert_eq!(
        headers.get("Content-Security-Policy"),
        Some("default-src 'self'")
    );
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

    assert_eq!(res.headers.get("X-Frame-Options"), Some("SAMEORIGIN"));
    assert_eq!(
        res.headers.get("Content-Security-Policy"),
        Some("default-src 'self' https:")
    );
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
    assert_eq!(res.headers.get("X-Frame-Options"), Some("SAMEORIGIN"));
    assert_eq!(res.headers.get("X-Content-Type-Options"), Some("nosniff"));
}
