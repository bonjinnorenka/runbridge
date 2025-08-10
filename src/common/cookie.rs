//! HTTPクッキー関連の実装

use std::fmt;
use std::time::Duration;
use chrono::{DateTime, Utc};
use crate::error::Error;
use super::utils::{validate_cookie_name_value, is_header_value_valid};

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
#[derive(Debug, Clone)]
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
    /// 新しいクッキーを作成（無効な文字は拒否）
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        // 互換API: 無効な値はパニックせずログに出してデフォルト無害値に置換
        // より厳密な扱いが必要な場合は `try_new` を使用
        match Self::try_new(name, value) {
            Ok(c) => c,
            Err(e) => {
                log::warn!("Cookie::new received invalid name/value: {}. Replaced with safe defaults", e);
                Self {
                    name: "invalid".to_string(),
                    value: "".to_string(),
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
        let n = name.into();
        let v = value.into();
        validate_cookie_name_value(&n, &v)?;
        Ok(Self {
            name: n,
            value: v,
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
            } else {
                log::warn!("Cookie::to_header_value skipped invalid Path value: {:?}", path);
            }
        }

        if let Some(domain) = &self.domain {
            if is_header_value_valid(domain) {
                cookie_str.push_str(&format!("; Domain={}", domain));
            } else {
                log::warn!("Cookie::to_header_value skipped invalid Domain value: {:?}", domain);
            }
        }

        if let Some(expires) = &self.expires {
            cookie_str.push_str(&format!("; Expires={}", expires.format("%a, %d %b %Y %H:%M:%S GMT")));
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
        let cookie = Cookie::new("expires_test", "value")
            .with_expires(expires);

        let header_value = cookie.to_header_value();
        assert!(header_value.contains("Expires=Tue, 31 Dec 2024 23:59:59 GMT"));
    }

    #[test]
    fn test_cookie_with_max_age() {
        let max_age = Duration::from_secs(3600); // 1 hour
        let cookie = Cookie::new("max_age_test", "value")
            .with_max_age(max_age);

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
        // 許容
        let ok = Cookie::try_new("SID", "abcDEF123-_.:~").unwrap();
        assert_eq!(ok.name, "SID");
        assert_eq!(ok.value, "abcDEF123-_.:~");

        // 値に禁止記号（; , \n など）
        assert!(Cookie::try_new("SID", "bad;value").is_err());
        assert!(Cookie::try_new("SID", "bad,value").is_err());
        assert!(Cookie::try_new("SID", "bad\nvalue").is_err());

        // 名前に禁止文字（空白・セパレータ）
        assert!(Cookie::try_new("bad name", "v").is_err());
        assert!(Cookie::try_new("bad;name", "v").is_err());
    }

    #[test]
    fn test_cookie_to_header_skips_invalid_attrs() {
        let mut c = Cookie::try_new("A", "B").unwrap();
        // 無効なPath/DomainはCRLF拒否によりスキップされる
        c.path = Some("/ok".into());
        c.domain = Some("bad\r\ndomain".into());
        let hv = c.to_header_value();
        assert!(hv.contains("A=B"));
        assert!(hv.contains("Path=/ok"));
        assert!(!hv.contains("Domain=bad"));
    }
}
