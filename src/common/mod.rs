//! 共通の抽象化レイヤーとトレイト定義

// 各モジュールを宣言
pub mod cgi;
pub mod context;
pub mod cookie;
pub mod http;
pub mod traits;
pub mod utils;

// 公開API用のre-export
pub use context::RequestContext;
pub use cookie::{Cookie, SameSite};
pub use http::{Method, Request, Response, ResponseBuilder, StatusCode};
pub use traits::{Handler, Middleware};
pub use utils::{get_max_body_size, parse_query_string, percent_decode};

// CGI関連の公開API
#[cfg(feature = "cgi")]
pub use cgi::{extract_cookies, extract_env_var, extract_headers, set_cookie, set_cookies};

// 古いcgi_utilsモジュールとの互換性維持のためのre-export
#[cfg(feature = "cgi")]
pub mod cgi_utils {
    pub use super::cgi::*;
}
