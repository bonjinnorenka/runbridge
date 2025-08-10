//! ハンドラーの実装（分割モジュール）

pub mod response;
pub mod pattern;
pub mod body;
pub mod core;
pub mod builders;

pub use response::ResponseWrapper;
pub use core::{RouteHandler, AsyncRouteHandler};
pub use builders::{
    get, try_get, async_get, try_async_get,
    post, async_post,
    put, async_put,
    delete, async_delete,
    options, async_options,
};

#[cfg(test)]
mod tests;

