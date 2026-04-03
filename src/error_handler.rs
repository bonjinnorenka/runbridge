use std::future::Future;
use std::sync::Arc;

use async_trait::async_trait;

use crate::common::{Request, Response};
use crate::error::Error;

#[async_trait]
pub trait ErrorHandler: Send + Sync {
    async fn handle(&self, req: &Request, err: &Error) -> Response;
}

struct DefaultErrorHandler;

#[async_trait]
impl ErrorHandler for DefaultErrorHandler {
    async fn handle(&self, _req: &Request, err: &Error) -> Response {
        Response::from_error(err)
    }
}

pub(crate) fn default_error_handler() -> Arc<dyn ErrorHandler> {
    Arc::new(DefaultErrorHandler)
}

struct ClosureErrorHandler<F> {
    inner: F,
}

#[async_trait]
impl<F, Fut> ErrorHandler for ClosureErrorHandler<F>
where
    F: Fn(&Request, &Error) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Response> + Send,
{
    async fn handle(&self, req: &Request, err: &Error) -> Response {
        (self.inner)(req, err).await
    }
}

pub fn error_handler<F, Fut>(f: F) -> impl ErrorHandler
where
    F: Fn(&Request, &Error) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Response> + Send + 'static,
{
    ClosureErrorHandler { inner: f }
}
