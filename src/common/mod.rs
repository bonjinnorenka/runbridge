//! 共通の抽象化レイヤーとトレイト定義

pub mod cgi;
pub mod context;
pub mod cookie;
pub mod http;
pub mod traits;
pub mod utils;

pub use context::RequestContext;
pub use cookie::{parse_cookie_header, parse_cookie_strings, Cookie, SameSite};
pub use http::{HeaderMap, Method, QueryMap, Request, Response, ResponseBuilder, StatusCode};
pub use traits::{Handler, Middleware, Next};
pub use utils::{get_max_body_size, parse_query_string, percent_decode};

#[cfg(feature = "cgi")]
pub use cgi::{extract_cookies, extract_env_var, extract_headers, set_cookie, set_cookies};

#[cfg(feature = "cgi")]
pub mod cgi_utils {
    pub use super::cgi::*;
}
