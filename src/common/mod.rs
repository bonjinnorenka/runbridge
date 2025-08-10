//! 共通の抽象化レイヤーとトレイト定義

// 各モジュールを宣言
pub mod http;
pub mod context;
pub mod traits;
pub mod cookie;
pub mod utils;
pub mod cgi;

// 公開API用のre-export
pub use http::{StatusCode, Method, Request, Response, ResponseBuilder};
pub use context::RequestContext;
pub use traits::{Handler, Middleware};
pub use cookie::{SameSite, Cookie};
pub use utils::{percent_decode, parse_query_string, get_max_body_size};

// CGI関連の公開API
#[cfg(feature = "cgi")]
pub use cgi::{extract_env_var, extract_cookies, extract_headers, set_cookie, set_cookies};

// 古いcgi_utilsモジュールとの互換性維持のためのre-export
#[cfg(feature = "cgi")]
pub mod cgi_utils {
    pub use super::cgi::*;
} 
