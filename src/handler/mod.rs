//! ハンドラーの実装（分割モジュール）

pub mod body;
pub mod builders;
pub mod core;
pub mod extractors;
pub mod response;

pub use builders::{
    async_delete, async_fallback, async_get, async_options, async_post, async_put, delete,
    fallback, get, options, post, put, try_async_get, try_get,
};
pub use core::{route, try_route, Route, RouteMatch};
pub use extractors::{ExtractError, FromRequest, FromRequestParts, Json, Path, Query, RequestParts, State};
pub use response::IntoResponse;

#[cfg(test)]
mod tests;
