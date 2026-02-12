//! ハンドラーの実装（分割モジュール）

pub mod body;
pub mod builders;
pub mod core;
pub mod pattern;
pub mod response;

pub use builders::{
    async_delete, async_get, async_options, async_post, async_put, delete, get, options, post, put,
    try_async_get, try_get,
};
pub use core::{AsyncRouteHandler, RouteHandler};
pub use response::ResponseWrapper;

#[cfg(test)]
mod tests;
