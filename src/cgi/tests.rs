//! CGIモジュールのテスト

use super::error_logging::{
    gather_cgi_panic_context, is_sensitive_key_like, redact_query_string, redact_value_for_log,
};
use super::request::get_cgi_headers;
use super::response::write_response_to;
use super::validation::{is_valid_header_name, is_valid_header_value};
use crate::common::{get_max_body_size, parse_query_string, Cookie, Response};

#[test]
fn test_parse_query_string() {
    let params = parse_query_string("name=John&age=30&city=Tokyo");
    assert_eq!(params.get("name"), Some("John"));
    assert_eq!(params.get("age"), Some("30"));
    assert_eq!(params.get("city"), Some("Tokyo"));
}

#[test]
fn test_parse_query_string_url_encoding() {
    let params = parse_query_string(
        "name=%E3%81%82%E3%81%84%E3%81%86%E3%81%88%E3%81%8A&city=Tokyo%20Station&lang=ja%2Den",
    );
    assert_eq!(params.get("name"), Some("あいうえお"));
    assert_eq!(params.get("city"), Some("Tokyo Station"));
    assert_eq!(params.get("lang"), Some("ja-en"));
}

#[test]
fn test_get_cgi_headers() {
    use temp_env::with_vars;
    with_vars(
        [
            ("HTTP_CONTENT_TYPE", Some("application/json")),
            ("HTTP_X_CUSTOM_HEADER", Some("test value")),
            ("HTTP_X_AUTH_TOKEN", Some("secret-token")),
            ("CONTENT_LENGTH", Some("123")),
            ("UNRELATED_VAR", Some("should not be included")),
        ],
        || {
            let headers = get_cgi_headers();
            assert_eq!(headers.get("Content-Type"), Some("application/json"));
            assert_eq!(headers.get("X-Custom-Header"), Some("test value"));
            assert_eq!(headers.get("X-Auth-Token"), Some("secret-token"));
            assert_eq!(headers.get("Content-Length"), Some("123"));
            assert_eq!(headers.get("UNRELATED_VAR"), None);
        },
    );
}

#[test]
fn test_get_max_body_size_default() {
    use temp_env::with_vars;
    with_vars([("RUNBRIDGE_MAX_BODY_SIZE", None::<&str>)], || {
        assert_eq!(get_max_body_size(), 5 * 1024 * 1024);
    });
}

#[test]
fn test_get_max_body_size_custom() {
    use temp_env::with_vars;
    with_vars([("RUNBRIDGE_MAX_BODY_SIZE", Some("1048576"))], || {
        assert_eq!(get_max_body_size(), 1048576);
    });
}

#[test]
fn test_get_max_body_size_invalid_env() {
    use temp_env::with_vars;
    with_vars([("RUNBRIDGE_MAX_BODY_SIZE", Some("invalid"))], || {
        assert_eq!(get_max_body_size(), 5 * 1024 * 1024);
    });
}

#[test]
fn test_is_valid_header_name() {
    assert!(is_valid_header_name("Content-Type"));
    assert!(is_valid_header_name("X-Custom-Header"));
    assert!(!is_valid_header_name(""));
    assert!(!is_valid_header_name("Content\rType"));
    assert!(!is_valid_header_name("Content Type"));
}

#[test]
fn test_is_valid_header_value() {
    assert!(is_valid_header_value("text/html"));
    assert!(is_valid_header_value("application/json; charset=utf-8"));
    assert!(is_valid_header_value(""));
    assert!(!is_valid_header_value("text/html\r\nSet-Cookie: malicious"));
    assert!(!is_valid_header_value("value\x00with\x01control"));
}

#[test]
fn test_write_response_multiple_set_cookie_lines() {
    let response = Response::new(200)
        .with_header("Content-Type", "text/plain")
        .with_cookie(Cookie::new("a", "1").with_path("/"))
        .with_cookie(Cookie::new("b", "2").with_path("/").secure(true))
        .with_body(b"ok".to_vec());

    let mut buf: Vec<u8> = Vec::new();
    write_response_to(response, &mut buf).expect("write_response_to failed");
    let out = String::from_utf8(buf).expect("utf8");

    let set_cookie_lines: Vec<&str> = out
        .lines()
        .filter(|line| line.starts_with("Set-Cookie:"))
        .collect();
    assert_eq!(set_cookie_lines.len(), 2);
    assert!(out.contains("Set-Cookie: a=1; Path=/"));
    assert!(out.contains("Set-Cookie: b=2; Path=/; Secure"));
    assert!(out.contains("Content-Length: 2\r"));
    assert!(out.ends_with("\r\nok"));
}

#[test]
fn test_write_response_preserves_binary_body() {
    let binary_body = vec![0x00, 0xff, 0x10, 0x80];
    let response = Response::new(200)
        .with_header("Content-Type", "application/octet-stream")
        .with_body(binary_body.clone());

    let mut buf: Vec<u8> = Vec::new();
    write_response_to(response, &mut buf).expect("write_response_to failed");

    let separator = b"\r\n\r\n";
    let body_index = buf
        .windows(separator.len())
        .position(|window| window == separator)
        .map(|index| index + separator.len())
        .expect("header separator must exist");

    assert!(buf[..body_index]
        .windows(b"Content-Type: application/octet-stream".len())
        .any(|window| window == b"Content-Type: application/octet-stream"));
    assert_eq!(&buf[body_index..], binary_body.as_slice());
}

#[test]
fn test_redact_value_for_log() {
    assert_eq!(
        redact_value_for_log("CONTENT_TYPE", "application/json"),
        "application/json"
    );
    assert_eq!(
        redact_value_for_log("HTTP_AUTHORIZATION", "Bearer token123"),
        "***redacted***"
    );
    assert_eq!(
        redact_value_for_log("QUERY_STRING", "name=john&token=secret123"),
        "name=john&token=***redacted***"
    );
}

#[test]
fn test_is_sensitive_key_like() {
    assert!(is_sensitive_key_like("authorization"));
    assert!(is_sensitive_key_like("cookie"));
    assert!(is_sensitive_key_like("token"));
    assert!(!is_sensitive_key_like("content_type"));
}

#[test]
fn test_redact_query_string() {
    let redacted = redact_query_string("name=john&token=secret123&lang=ja");
    assert_eq!(redacted, "name=john&token=***redacted***&lang=ja");
}

#[test]
fn test_gather_cgi_panic_context() {
    use temp_env::with_vars;

    with_vars(
        [
            ("QUERY_STRING", Some("name=john&token=secret123")),
            ("CONTENT_TYPE", Some("application/json")),
            ("HTTP_AUTHORIZATION", Some("Bearer token")),
        ],
        || {
            let context = gather_cgi_panic_context("POST", "/api/test");
            assert!(context.contains("REQUEST_METHOD=POST"));
            assert!(context.contains("PATH_INFO=/api/test"));
            assert!(context.contains("application/json"));
            assert!(context.contains("***redacted***"));
        },
    );
}
