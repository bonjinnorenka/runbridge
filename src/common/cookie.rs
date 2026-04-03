//! HTTPクッキー関連の実装

use super::utils::{is_header_value_valid, validate_cookie_name_value};
use crate::error::Error;
use chrono::{DateTime, Utc};
use std::fmt;
use std::time::Duration;

/// SameSite属性
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SameSite {
    Strict,
    Lax,
    None,
}

impl fmt::Display for SameSite {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SameSite::Strict => write!(f, "Strict"),
            SameSite::Lax => write!(f, "Lax"),
            SameSite::None => write!(f, "None"),
        }
    }
}

/// HTTPクッキー
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cookie {
    pub name: String,
    pub value: String,
    pub path: Option<String>,
    pub domain: Option<String>,
    pub expires: Option<DateTime<Utc>>,
    pub max_age: Option<Duration>,
    pub secure: bool,
    pub http_only: bool,
    pub same_site: Option<SameSite>,
}

impl Cookie {
    /// 新しいクッキーを作成（無効な文字は安全なデフォルトへフォールバック）
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        match Self::try_new(name, value) {
            Ok(cookie) => cookie,
            Err(err) => {
                log::warn!(
                    "Cookie::new received invalid name/value: {}. Replaced with safe defaults",
                    err
                );
                Self {
                    name: "invalid".to_string(),
                    value: String::new(),
                    path: None,
                    domain: None,
                    expires: None,
                    max_age: None,
                    secure: false,
                    http_only: false,
                    same_site: None,
                }
            }
        }
    }

    /// 新しいクッキーをResultで作成（推奨）
    pub fn try_new(name: impl Into<String>, value: impl Into<String>) -> Result<Self, Error> {
        let name = name.into();
        let value = value.into();
        validate_cookie_name_value(&name, &value)?;
        Ok(Self {
            name,
            value,
            path: None,
            domain: None,
            expires: None,
            max_age: None,
            secure: false,
            http_only: false,
            same_site: None,
        })
    }

    /// パスを設定
    pub fn with_path(mut self, path: impl Into<String>) -> Self {
        self.path = Some(path.into());
        self
    }

    /// ドメインを設定
    pub fn with_domain(mut self, domain: impl Into<String>) -> Self {
        self.domain = Some(domain.into());
        self
    }

    /// 有効期限を設定
    pub fn with_expires(mut self, expires: DateTime<Utc>) -> Self {
        self.expires = Some(expires);
        self
    }

    /// 最大年齢を設定
    pub fn with_max_age(mut self, max_age: Duration) -> Self {
        self.max_age = Some(max_age);
        self
    }

    /// セキュアフラグを設定
    pub fn secure(mut self, secure: bool) -> Self {
        self.secure = secure;
        self
    }

    /// HttpOnlyフラグを設定
    pub fn http_only(mut self, http_only: bool) -> Self {
        self.http_only = http_only;
        self
    }

    /// SameSite属性を設定
    pub fn with_same_site(mut self, same_site: SameSite) -> Self {
        self.same_site = Some(same_site);
        self
    }

    /// Set-Cookieヘッダー値を生成
    pub fn to_header_value(&self) -> String {
        let mut cookie_str = format!("{}={}", self.name, self.value);

        if let Some(path) = &self.path {
            if is_header_value_valid(path) {
                cookie_str.push_str(&format!("; Path={}", path));
            }
        }

        if let Some(domain) = &self.domain {
            if is_header_value_valid(domain) {
                cookie_str.push_str(&format!("; Domain={}", domain));
            }
        }

        if let Some(expires) = &self.expires {
            cookie_str.push_str(&format!(
                "; Expires={}",
                expires.format("%a, %d %b %Y %H:%M:%S GMT")
            ));
        }

        if let Some(max_age) = &self.max_age {
            cookie_str.push_str(&format!("; Max-Age={}", max_age.as_secs()));
        }

        if self.secure {
            cookie_str.push_str("; Secure");
        }

        if self.http_only {
            cookie_str.push_str("; HttpOnly");
        }

        if let Some(same_site) = &self.same_site {
            cookie_str.push_str(&format!("; SameSite={}", same_site));
        }

        cookie_str
    }
}

/// Cookie ヘッダー文字列を request cookie にパース
pub fn parse_cookie_header(value: &str) -> Vec<Cookie> {
    value
        .split(';')
        .filter_map(|part| {
            let trimmed = part.trim();
            let (name, value) = trimmed.split_once('=')?;
            Cookie::try_new(name.trim(), value.trim()).ok()
        })
        .collect()
}

/// Cookie 文字列の配列を request cookie にパース
pub fn parse_cookie_strings(values: impl IntoIterator<Item = impl AsRef<str>>) -> Vec<Cookie> {
    let mut cookies = Vec::new();
    for value in values {
        let value = value.as_ref();
        if value.contains(';') {
            cookies.extend(parse_cookie_header(value));
        } else if let Some((name, value)) = value.split_once('=') {
            if let Ok(cookie) = Cookie::try_new(name.trim(), value.trim()) {
                cookies.push(cookie);
            }
        }
    }
    cookies
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cookie_basic() {
        let cookie = Cookie::new("session_id", "abc123");

        assert_eq!(cookie.name, "session_id");
        assert_eq!(cookie.value, "abc123");
        assert_eq!(cookie.path, None);
        assert_eq!(cookie.domain, None);
        assert!(!cookie.secure);
        assert!(!cookie.http_only);
        assert_eq!(cookie.same_site, None);
    }

    #[test]
    fn test_cookie_builder() {
        let cookie = Cookie::new("auth_token", "xyz789")
            .with_path("/")
            .with_domain("example.com")
            .secure(true)
            .http_only(true)
            .with_same_site(SameSite::Strict);

        assert_eq!(cookie.path, Some("/".to_string()));
        assert_eq!(cookie.domain, Some("example.com".to_string()));
        assert!(cookie.secure);
        assert!(cookie.http_only);
        assert_eq!(cookie.same_site, Some(SameSite::Strict));
    }

    #[test]
    fn test_cookie_header_value() {
        let cookie = Cookie::new("test", "value")
            .with_path("/app")
            .with_domain("test.com")
            .secure(true)
            .http_only(true)
            .with_same_site(SameSite::Lax);

        let header_value = cookie.to_header_value();

        assert!(header_value.contains("test=value"));
        assert!(header_value.contains("Path=/app"));
        assert!(header_value.contains("Domain=test.com"));
        assert!(header_value.contains("Secure"));
        assert!(header_value.contains("HttpOnly"));
        assert!(header_value.contains("SameSite=Lax"));
    }

    #[test]
    fn test_cookie_with_expires() {
        use chrono::{TimeZone, Utc};

        let expires = Utc.with_ymd_and_hms(2024, 12, 31, 23, 59, 59).unwrap();
        let cookie = Cookie::new("expires_test", "value").with_expires(expires);

        let header_value = cookie.to_header_value();
        assert!(header_value.contains("Expires=Tue, 31 Dec 2024 23:59:59 GMT"));
    }

    #[test]
    fn test_cookie_with_max_age() {
        let max_age = Duration::from_secs(3600);
        let cookie = Cookie::new("max_age_test", "value").with_max_age(max_age);

        let header_value = cookie.to_header_value();
        assert!(header_value.contains("Max-Age=3600"));
    }

    #[test]
    fn test_same_site_display() {
        assert_eq!(SameSite::Strict.to_string(), "Strict");
        assert_eq!(SameSite::Lax.to_string(), "Lax");
        assert_eq!(SameSite::None.to_string(), "None");
    }

    #[test]
    fn test_cookie_try_new_validation() {
        assert!(Cookie::try_new("good_name", "good-value").is_ok());
        assert!(Cookie::try_new("bad name", "good-value").is_err());
        assert!(Cookie::try_new("good_name", "bad;value").is_err());
    }

    #[test]
    fn test_cookie_to_header_skips_invalid_attrs() {
        let cookie = Cookie::new("t", "v")
            .with_path("/ok")
            .with_domain("bad\r\nvalue");
        let header_value = cookie.to_header_value();
        assert!(header_value.contains("Path=/ok"));
        assert!(!header_value.contains("Domain="));
    }

    #[test]
    fn test_parse_cookie_header() {
        let cookies = parse_cookie_header("session=abc123; theme=dark");
        assert_eq!(cookies.len(), 2);
        assert_eq!(cookies[0].name, "session");
        assert_eq!(cookies[1].name, "theme");
    }

    #[test]
    fn test_parse_cookie_strings() {
        let cookies = parse_cookie_strings(vec!["session=abc123", "theme=dark"]);
        assert_eq!(cookies.len(), 2);
        assert_eq!(cookies[0].value, "abc123");
        assert_eq!(cookies[1].value, "dark");
    }
}
