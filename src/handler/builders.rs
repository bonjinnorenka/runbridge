use std::future::Future;

use futures::future::{self, Ready};
use serde::de::DeserializeOwned;

use crate::common::Method;
use crate::common::Request;
use crate::error::Error;

use super::core::{AsyncRouteHandler, RouteHandler};
use super::response::ResponseWrapper;

// 可読性のための型エイリアス（ボディ必須の非同期ハンドラー）
pub type BodyOrError<Fut, R> = future::Either<Ready<Result<R, Error>>, Fut>;

// 同期: Option<T> から T を要求し、なければエラーにする薄いアダプタ
fn require_body_sync<F, T, R>(handler: F) -> impl Fn(Request, Option<T>) -> Result<R, Error> + Send + Sync + 'static
where
    F: Fn(Request, T) -> Result<R, Error> + Send + Sync + 'static,
    T: serde::de::DeserializeOwned + Send + Sync + 'static,
    R: ResponseWrapper + Send + Sync + 'static,
{
    move |req, body_data| {
        if let Some(data) = body_data {
            handler(req, data)
        } else {
            Err(Error::InvalidRequestBody("Missing request body".to_string()))
        }
    }
}

// 非同期: Option<T> から T を要求し、なければ即時エラーfutureを返すアダプタ
fn require_body_async<F, T, R, Fut>(handler: F) -> impl Fn(Request, Option<T>) -> BodyOrError<Fut, R> + Send + Sync + 'static
where
    F: Fn(Request, T) -> Fut + Send + Sync + 'static,
    T: serde::de::DeserializeOwned + Send + Sync + 'static,
    R: ResponseWrapper + Send + Sync + 'static,
    Fut: Future<Output = Result<R, Error>> + Send + Sync + 'static,
{
    move |req, body_data| {
        if let Some(data) = body_data {
            future::Either::Right(handler(req, data))
        } else {
            future::Either::Left(future::ready(Err(Error::InvalidRequestBody(
                "Missing request body".to_string(),
            ))))
        }
    }
}

/// マクロでHTTPハンドラーを生成するための補助関数
pub fn get<F, R>(path: impl Into<String>, handler: F) -> RouteHandler<impl Fn(Request, Option<()>) -> Result<R, Error> + Send + Sync + 'static, (), R>
where
    F: Fn(Request) -> Result<R, Error> + Send + Sync + 'static,
    R: ResponseWrapper + Send + Sync + 'static,
{
    #[allow(deprecated)]
    RouteHandler::new(Method::GET, path, move |req, _| handler(req))
}

/// マクロでHTTPハンドラーを生成するための補助関数（エラーハンドリング付き）
pub fn try_get<F, R>(path: impl Into<String>, handler: F) -> Result<RouteHandler<impl Fn(Request, Option<()>) -> Result<R, Error> + Send + Sync + 'static, (), R>, Error>
where
    F: Fn(Request) -> Result<R, Error> + Send + Sync + 'static,
    R: ResponseWrapper + Send + Sync + 'static,
{
    RouteHandler::try_new(Method::GET, path, move |req, _| handler(req))
}

/// 非同期GETハンドラーを作成
pub fn async_get<F, R, Fut>(path: impl Into<String>, handler: F) -> AsyncRouteHandler<impl Fn(Request, Option<()>) -> Fut + Send + Sync + 'static, (), R, Fut>
where
    F: Fn(Request) -> Fut + Send + Sync + 'static,
    R: ResponseWrapper + Send + Sync + 'static,
    Fut: Future<Output = Result<R, Error>> + Send + Sync + 'static,
{
    #[allow(deprecated)]
    AsyncRouteHandler::new(Method::GET, path, move |req, _| handler(req))
}

/// 非同期GETハンドラーを作成（エラーハンドリング付き）
pub fn try_async_get<F, R, Fut>(path: impl Into<String>, handler: F) -> Result<AsyncRouteHandler<impl Fn(Request, Option<()>) -> Fut + Send + Sync + 'static, (), R, Fut>, Error>
where
    F: Fn(Request) -> Fut + Send + Sync + 'static,
    R: ResponseWrapper + Send + Sync + 'static,
    Fut: Future<Output = Result<R, Error>> + Send + Sync + 'static,
{
    AsyncRouteHandler::try_new(Method::GET, path, move |req, _| handler(req))
}

/// POSTハンドラーを作成
pub fn post<F, T, R>(path: impl Into<String>, handler: F) -> RouteHandler<impl Fn(Request, Option<T>) -> Result<R, Error> + Send + Sync + 'static, T, R>
where
    F: Fn(Request, T) -> Result<R, Error> + Send + Sync + 'static,
    T: DeserializeOwned + Send + Sync + 'static,
    R: ResponseWrapper + Send + Sync + 'static,
{
    #[allow(deprecated)]
    RouteHandler::new(Method::POST, path, require_body_sync(handler))
}

/// 非同期POSTハンドラーを作成
pub fn async_post<F, T, R, Fut>(path: impl Into<String>, handler: F) -> AsyncRouteHandler<impl Fn(Request, Option<T>) -> BodyOrError<Fut, R> + Send + Sync + 'static, T, R, BodyOrError<Fut, R>>
where
    F: Fn(Request, T) -> Fut + Send + Sync + 'static,
    T: DeserializeOwned + Send + Sync + 'static,
    R: ResponseWrapper + Send + Sync + 'static,
    Fut: Future<Output = Result<R, Error>> + Send + Sync + 'static,
{
    #[allow(deprecated)]
    AsyncRouteHandler::new(Method::POST, path, require_body_async(handler))
}

/// PUTハンドラーを作成
pub fn put<F, T, R>(path: impl Into<String>, handler: F) -> RouteHandler<impl Fn(Request, Option<T>) -> Result<R, Error> + Send + Sync + 'static, T, R>
where
    F: Fn(Request, T) -> Result<R, Error> + Send + Sync + 'static,
    T: DeserializeOwned + Send + Sync + 'static,
    R: ResponseWrapper + Send + Sync + 'static,
{
    #[allow(deprecated)]
    RouteHandler::new(Method::PUT, path, require_body_sync(handler))
}

/// 非同期PUTハンドラーを作成
pub fn async_put<F, T, R, Fut>(path: impl Into<String>, handler: F) -> AsyncRouteHandler<impl Fn(Request, Option<T>) -> BodyOrError<Fut, R> + Send + Sync + 'static, T, R, BodyOrError<Fut, R>>
where
    F: Fn(Request, T) -> Fut + Send + Sync + 'static,
    T: DeserializeOwned + Send + Sync + 'static,
    R: ResponseWrapper + Send + Sync + 'static,
    Fut: Future<Output = Result<R, Error>> + Send + Sync + 'static,
{
    #[allow(deprecated)]
    AsyncRouteHandler::new(Method::PUT, path, require_body_async(handler))
}

/// DELETEハンドラーを作成
pub fn delete<F, R>(path: impl Into<String>, handler: F) -> RouteHandler<impl Fn(Request, Option<()>) -> Result<R, Error> + Send + Sync + 'static, (), R>
where
    F: Fn(Request) -> Result<R, Error> + Send + Sync + 'static,
    R: ResponseWrapper + Send + Sync + 'static,
{
    #[allow(deprecated)]
    RouteHandler::new(Method::DELETE, path, move |req, _| handler(req))
}

/// 非同期DELETEハンドラーを作成
pub fn async_delete<F, R, Fut>(path: impl Into<String>, handler: F) -> AsyncRouteHandler<impl Fn(Request, Option<()>) -> Fut + Send + Sync + 'static, (), R, Fut>
where
    F: Fn(Request) -> Fut + Send + Sync + 'static,
    R: ResponseWrapper + Send + Sync + 'static,
    Fut: Future<Output = Result<R, Error>> + Send + Sync + 'static,
{
    #[allow(deprecated)]
    AsyncRouteHandler::new(Method::DELETE, path, move |req, _| handler(req))
}

/// OPTIONSハンドラーを作成
pub fn options<F, R>(path: impl Into<String>, handler: F) -> RouteHandler<impl Fn(Request, Option<()>) -> Result<R, Error> + Send + Sync + 'static, (), R>
where
    F: Fn(Request) -> Result<R, Error> + Send + Sync + 'static,
    R: ResponseWrapper + Send + Sync + 'static,
{
    #[allow(deprecated)]
    RouteHandler::new(Method::OPTIONS, path, move |req, _| handler(req))
}

/// 非同期OPTIONSハンドラーを作成
pub fn async_options<F, R, Fut>(path: impl Into<String>, handler: F) -> AsyncRouteHandler<impl Fn(Request, Option<()>) -> Fut + Send + Sync + 'static, (), R, Fut>
where
    F: Fn(Request) -> Fut + Send + Sync + 'static,
    R: ResponseWrapper + Send + Sync + 'static,
    Fut: Future<Output = Result<R, Error>> + Send + Sync + 'static,
{
    #[allow(deprecated)]
    AsyncRouteHandler::new(Method::OPTIONS, path, move |req, _| handler(req))
}
