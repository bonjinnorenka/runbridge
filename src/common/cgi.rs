//! CGI環境向けのユーティリティ機能

#[cfg(feature = "cgi")]
pub mod utils {
    use super::super::cookie::{parse_cookie_header, Cookie};
    use super::super::http::{HeaderMap, Response};
    use std::env;

    pub fn extract_env_var(key: &str) -> Option<String> {
        env::var(key).ok()
    }

    pub fn extract_cookies() -> Vec<Cookie> {
        extract_env_var("HTTP_COOKIE")
            .map(|cookie_header| parse_cookie_header(&cookie_header))
            .unwrap_or_default()
    }

    pub fn extract_headers() -> HeaderMap {
        let mut headers = HeaderMap::new();

        for (key, value) in env::vars() {
            if key.starts_with("HTTP_") {
                let header_name = key[5..].replace('_', "-").to_lowercase();
                headers.append(header_name, value);
            }
        }

        if let Some(content_type) = extract_env_var("CONTENT_TYPE") {
            headers.append("content-type", content_type);
        }

        if let Some(content_length) = extract_env_var("CONTENT_LENGTH") {
            headers.append("content-length", content_length);
        }

        headers
    }

    pub fn set_cookie(response: &mut Response, cookie: Cookie) {
        response.cookies.push(cookie);
    }

    pub fn set_cookies(response: &mut Response, cookies: Vec<Cookie>) {
        response.cookies.extend(cookies);
    }
}

#[cfg(feature = "cgi")]
pub use utils::*;

#[cfg(test)]
#[cfg(feature = "cgi")]
mod tests {
    use super::super::cookie::Cookie;
    use super::super::http::Response;
    use super::*;
    use std::env;

    #[test]
    fn test_cgi_utils_mock() {
        env::set_var("HTTP_COOKIE", "session=abc123; user_id=456");
        env::set_var("HTTP_USER_AGENT", "TestAgent/1.0");
        env::set_var("CONTENT_TYPE", "application/json");

        let cookies = extract_cookies();
        assert_eq!(cookies[0].name, "session");
        assert_eq!(cookies[1].value, "456");

        let headers = extract_headers();
        assert_eq!(headers.get("user-agent"), Some("TestAgent/1.0"));
        assert_eq!(headers.get("content-type"), Some("application/json"));

        env::remove_var("HTTP_COOKIE");
        env::remove_var("HTTP_USER_AGENT");
        env::remove_var("CONTENT_TYPE");
    }

    #[test]
    fn test_set_cookie_response() {
        let mut response = Response::new(200);
        let cookie = Cookie::new("test_cookie", "test_value")
            .with_path("/")
            .secure(true);

        set_cookie(&mut response, cookie);

        assert_eq!(response.cookies.len(), 1);
        assert!(response.cookies[0].to_header_value().contains("test_cookie=test_value"));
    }
}
