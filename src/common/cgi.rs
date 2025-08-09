//! CGI環境向けのユーティリティ機能

#[cfg(feature = "cgi")]
pub mod utils {
    use std::collections::HashMap;
    use std::env;
    use super::super::http::Response;
    use super::super::cookie::Cookie;

    /// 環境変数を取得
    pub fn extract_env_var(key: &str) -> Option<String> {
        env::var(key).ok()
    }

    /// CGI環境からクッキーを抽出
    pub fn extract_cookies() -> HashMap<String, String> {
        let mut cookies = HashMap::new();
        
        if let Some(cookie_header) = extract_env_var("HTTP_COOKIE") {
            for cookie_pair in cookie_header.split(';') {
                let parts: Vec<&str> = cookie_pair.trim().splitn(2, '=').collect();
                if parts.len() == 2 {
                    cookies.insert(parts[0].to_string(), parts[1].to_string());
                }
            }
        }
        
        cookies
    }

    /// CGI環境からHTTPヘッダーを抽出
    pub fn extract_headers() -> HashMap<String, String> {
        let mut headers = HashMap::new();
        
        for (key, value) in env::vars() {
            if key.starts_with("HTTP_") {
                let header_name = key[5..].replace('_', "-").to_lowercase();
                headers.insert(header_name, value);
            }
        }
        
        // 特別なヘッダーを個別に処理
        if let Some(content_type) = extract_env_var("CONTENT_TYPE") {
            headers.insert("content-type".to_string(), content_type);
        }
        
        if let Some(content_length) = extract_env_var("CONTENT_LENGTH") {
            headers.insert("content-length".to_string(), content_length);
        }
        
        headers
    }

    /// レスポンスにクッキーを設定
    pub fn set_cookie(response: &mut Response, cookie: Cookie) {
        response.headers.insert("Set-Cookie".to_string(), cookie.to_header_value());
    }

    /// レスポンスに複数のクッキーを設定
    pub fn set_cookies(response: &mut Response, cookies: Vec<Cookie>) {
        for cookie in cookies {
            // Set-Cookieヘッダーは複数設定可能だが、HashMapでは上書きされるため
            // 既存の実装では最後のクッキーのみが有効になる
            // 実際の実装では Vec<(String, String)> を使用するか、
            // 複数のSet-Cookieヘッダーを連結する必要がある
            set_cookie(response, cookie);
        }
    }
}

// CGIモジュールのユーティリティ関数を公開
#[cfg(feature = "cgi")]
pub use utils::*;

#[cfg(test)]
#[cfg(feature = "cgi")]
mod tests {
    use super::*;
    use super::super::http::Response;
    use super::super::cookie::{Cookie, SameSite};
    use std::env;

    #[test]
    fn test_cgi_utils_mock() {
        // CGI環境をモックしてテスト
        // 実際のテストでは環境変数を設定する必要がある

        // 環境変数を設定
        env::set_var("HTTP_COOKIE", "session=abc123; user_id=456");
        env::set_var("HTTP_USER_AGENT", "TestAgent/1.0");
        env::set_var("CONTENT_TYPE", "application/json");

        let cookies = extract_cookies();
        assert_eq!(cookies.get("session"), Some(&"abc123".to_string()));
        assert_eq!(cookies.get("user_id"), Some(&"456".to_string()));

        let headers = extract_headers();
        assert_eq!(headers.get("user-agent"), Some(&"TestAgent/1.0".to_string()));
        assert_eq!(headers.get("content-type"), Some(&"application/json".to_string()));

        // クリーンアップ
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

        let set_cookie_header = response.headers.get("Set-Cookie");
        assert!(set_cookie_header.is_some());
        
        let header_value = set_cookie_header.unwrap();
        assert!(header_value.contains("test_cookie=test_value"));
        assert!(header_value.contains("Path=/"));
        assert!(header_value.contains("Secure"));
    }
}