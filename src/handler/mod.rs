//! ハンドラーの実装（分割モジュール）

pub mod body;
pub mod builders;
pub mod core;
pub mod extractors;
pub mod fixed_file;
pub mod response;

pub use crate::router::Router;
pub use builders::{
    async_delete, async_fallback, async_get, async_head, async_options, async_patch,
    async_patch_bytes, async_patch_form, async_patch_text, async_post, async_post_bytes,
    async_post_form, async_post_text, async_put, async_put_bytes, async_put_form, async_put_text,
    delete, fallback, get, head, options, patch, patch_bytes, patch_form, patch_text, post,
    post_bytes, post_form, post_text, put, put_bytes, put_form, put_text, try_async_get,
    try_async_head, try_async_patch, try_get, try_head, try_patch,
};
pub use core::{route, try_route, Route, RouteMatch};
pub use extractors::{
    BytesBody, ExtractError, Form, FromRequest, FromRequestParts, Json, Path, Query, RequestParts,
    State, TextBody,
};
pub use fixed_file::FixedFileOptions;
pub use response::IntoResponse;

#[cfg(test)]
mod tests;
