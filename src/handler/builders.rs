use std::future::Future;
use std::sync::Arc;

use serde::de::DeserializeOwned;

use crate::common::{Handler, Method, Request};
use crate::error::Error;

use super::core::{async_handler, route, sync_handler, try_route, Route};
use super::extractors::{FromRequest, Json};
use super::IntoResponse;

fn sync_json_handler<F, T, R>(handler: F) -> impl Handler
where
    F: Fn(Request, T) -> Result<R, Error> + Send + Sync + 'static,
    T: DeserializeOwned + Send + 'static,
    R: IntoResponse + 'static,
{
    let handler = Arc::new(handler);
    async_handler(move |req: Request| {
        let handler = Arc::clone(&handler);
        let req_for_body = req.clone_without_context();
        let fut = async move {
            let body = match Json::<T>::from_request(&req_for_body).await {
                Ok(body) => body.0,
                Err(rejection) => return Ok(rejection.into_response()),
            };
            handler(req, body).map(|value| value.into_response())
        };
        fut
    })
}

fn async_json_handler<F, T, R, Fut>(handler: F) -> impl Handler
where
    F: Fn(Request, T) -> Fut + Send + Sync + 'static,
    T: DeserializeOwned + Send + 'static,
    R: IntoResponse + 'static,
    Fut: Future<Output = Result<R, Error>> + Send + 'static,
{
    let handler = Arc::new(handler);
    async_handler(move |req: Request| {
        let handler = Arc::clone(&handler);
        let req_for_body = req.clone_without_context();
        let fut = async move {
            let body = match Json::<T>::from_request(&req_for_body).await {
                Ok(body) => body.0,
                Err(rejection) => return Ok(rejection.into_response()),
            };
            handler(req, body).await.map(|value| value.into_response())
        };
        fut
    })
}

pub fn get<F, R>(path: impl Into<String>, handler: F) -> Route
where
    F: Fn(Request) -> Result<R, Error> + Send + Sync + 'static,
    R: IntoResponse + 'static,
{
    route(Method::GET, path, sync_handler(handler))
}

pub fn try_get<F, R>(path: impl Into<String>, handler: F) -> Result<Route, Error>
where
    F: Fn(Request) -> Result<R, Error> + Send + Sync + 'static,
    R: IntoResponse + 'static,
{
    try_route(Method::GET, path, sync_handler(handler))
}

pub fn async_get<F, R, Fut>(path: impl Into<String>, handler: F) -> Route
where
    F: Fn(Request) -> Fut + Send + Sync + 'static,
    R: IntoResponse + 'static,
    Fut: Future<Output = Result<R, Error>> + Send + 'static,
{
    route(Method::GET, path, async_handler(handler))
}

pub fn try_async_get<F, R, Fut>(path: impl Into<String>, handler: F) -> Result<Route, Error>
where
    F: Fn(Request) -> Fut + Send + Sync + 'static,
    R: IntoResponse + 'static,
    Fut: Future<Output = Result<R, Error>> + Send + 'static,
{
    try_route(Method::GET, path, async_handler(handler))
}

pub fn post<F, T, R>(path: impl Into<String>, handler: F) -> Route
where
    F: Fn(Request, T) -> Result<R, Error> + Send + Sync + 'static,
    T: DeserializeOwned + Send + 'static,
    R: IntoResponse + 'static,
{
    route(Method::POST, path, sync_json_handler(handler))
}

pub fn async_post<F, T, R, Fut>(path: impl Into<String>, handler: F) -> Route
where
    F: Fn(Request, T) -> Fut + Send + Sync + 'static,
    T: DeserializeOwned + Send + 'static,
    R: IntoResponse + 'static,
    Fut: Future<Output = Result<R, Error>> + Send + 'static,
{
    route(Method::POST, path, async_json_handler(handler))
}

pub fn put<F, T, R>(path: impl Into<String>, handler: F) -> Route
where
    F: Fn(Request, T) -> Result<R, Error> + Send + Sync + 'static,
    T: DeserializeOwned + Send + 'static,
    R: IntoResponse + 'static,
{
    route(Method::PUT, path, sync_json_handler(handler))
}

pub fn async_put<F, T, R, Fut>(path: impl Into<String>, handler: F) -> Route
where
    F: Fn(Request, T) -> Fut + Send + Sync + 'static,
    T: DeserializeOwned + Send + 'static,
    R: IntoResponse + 'static,
    Fut: Future<Output = Result<R, Error>> + Send + 'static,
{
    route(Method::PUT, path, async_json_handler(handler))
}

pub fn delete<F, R>(path: impl Into<String>, handler: F) -> Route
where
    F: Fn(Request) -> Result<R, Error> + Send + Sync + 'static,
    R: IntoResponse + 'static,
{
    route(Method::DELETE, path, sync_handler(handler))
}

pub fn async_delete<F, R, Fut>(path: impl Into<String>, handler: F) -> Route
where
    F: Fn(Request) -> Fut + Send + Sync + 'static,
    R: IntoResponse + 'static,
    Fut: Future<Output = Result<R, Error>> + Send + 'static,
{
    route(Method::DELETE, path, async_handler(handler))
}

pub fn options<F, R>(path: impl Into<String>, handler: F) -> Route
where
    F: Fn(Request) -> Result<R, Error> + Send + Sync + 'static,
    R: IntoResponse + 'static,
{
    route(Method::OPTIONS, path, sync_handler(handler))
}

pub fn async_options<F, R, Fut>(path: impl Into<String>, handler: F) -> Route
where
    F: Fn(Request) -> Fut + Send + Sync + 'static,
    R: IntoResponse + 'static,
    Fut: Future<Output = Result<R, Error>> + Send + 'static,
{
    route(Method::OPTIONS, path, async_handler(handler))
}

pub fn fallback<F, R>(handler: F) -> impl Handler
where
    F: Fn(Request) -> Result<R, Error> + Send + Sync + 'static,
    R: IntoResponse + 'static,
{
    sync_handler(handler)
}

pub fn async_fallback<F, R, Fut>(handler: F) -> impl Handler
where
    F: Fn(Request) -> Fut + Send + Sync + 'static,
    R: IntoResponse + 'static,
    Fut: Future<Output = Result<R, Error>> + Send + 'static,
{
    async_handler(handler)
}
