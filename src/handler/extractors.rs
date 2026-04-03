use std::collections::HashMap;
use std::ops::Deref;
use std::sync::Arc;

use async_trait::async_trait;
use serde::de::DeserializeOwned;

use crate::common::{Cookie, HeaderMap, Method, QueryMap, Request, RequestContext, Response};

use super::body::is_json_like_content_type;
use super::response::IntoResponse;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExtractErrorKind {
    BadRequest,
    InternalServerError,
}

/// extractor の固定 rejection
#[derive(Debug, Clone)]
pub struct ExtractError {
    kind: ExtractErrorKind,
    message: String,
}

impl ExtractError {
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self {
            kind: ExtractErrorKind::BadRequest,
            message: message.into(),
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self {
            kind: ExtractErrorKind::InternalServerError,
            message: message.into(),
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl IntoResponse for ExtractError {
    fn into_response(self) -> Response {
        match self.kind {
            ExtractErrorKind::BadRequest => Response::bad_request()
                .with_header("Content-Type", "text/plain")
                .with_body("Bad Request".as_bytes().to_vec()),
            ExtractErrorKind::InternalServerError => Response::internal_server_error()
                .with_header("Content-Type", "text/plain")
                .with_body("Internal Server Error".as_bytes().to_vec()),
        }
    }
}

/// body を含まない request parts
pub struct RequestParts<'a> {
    pub method: &'a Method,
    pub path: &'a str,
    pub query: &'a QueryMap,
    pub headers: &'a HeaderMap,
    pub path_params: &'a HashMap<String, String>,
    pub cookies: &'a [Cookie],
    pub context: &'a RequestContext,
}

impl<'a> From<&'a Request> for RequestParts<'a> {
    fn from(req: &'a Request) -> Self {
        Self {
            method: &req.method,
            path: &req.path,
            query: &req.query,
            headers: &req.headers,
            path_params: &req.path_params,
            cookies: &req.cookies,
            context: req.context(),
        }
    }
}

#[async_trait]
pub trait FromRequestParts: Sized {
    type Rejection: IntoResponse;

    async fn from_request_parts(parts: &RequestParts<'_>) -> Result<Self, Self::Rejection>;
}

#[async_trait]
pub trait FromRequest: Sized {
    type Rejection: IntoResponse;

    async fn from_request(req: &Request) -> Result<Self, Self::Rejection>;
}

#[async_trait]
impl FromRequest for Request {
    type Rejection = ExtractError;

    async fn from_request(req: &Request) -> Result<Self, Self::Rejection> {
        Ok(req.clone_without_context())
    }
}

/// `Path<T>` extractor
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Path<T>(pub T);

impl<T> Deref for Path<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[async_trait]
impl<T> FromRequestParts for Path<T>
where
    T: DeserializeOwned + Send,
{
    type Rejection = ExtractError;

    async fn from_request_parts(parts: &RequestParts<'_>) -> Result<Self, Self::Rejection> {
        let encoded = serde_urlencoded::to_string(parts.path_params)
            .map_err(|err| ExtractError::bad_request(err.to_string()))?;
        let value = serde_urlencoded::from_str(&encoded)
            .map_err(|err| ExtractError::bad_request(err.to_string()))?;
        Ok(Self(value))
    }
}

/// `Query<T>` extractor
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Query<T>(pub T);

impl<T> Deref for Query<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[async_trait]
impl<T> FromRequestParts for Query<T>
where
    T: DeserializeOwned + Send,
{
    type Rejection = ExtractError;

    async fn from_request_parts(parts: &RequestParts<'_>) -> Result<Self, Self::Rejection> {
        let encoded = serde_urlencoded::to_string(parts.query.to_owned_pairs())
            .map_err(|err| ExtractError::bad_request(err.to_string()))?;
        let value = serde_html_form::from_str(&encoded)
            .map_err(|err| ExtractError::bad_request(err.to_string()))?;
        Ok(Self(value))
    }
}

/// `State<T>` extractor
#[derive(Debug, Clone)]
pub struct State<T>(pub Arc<T>);

impl<T> Deref for State<T> {
    type Target = Arc<T>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[async_trait]
impl<T> FromRequestParts for State<T>
where
    T: Send + Sync + 'static,
{
    type Rejection = ExtractError;

    async fn from_request_parts(parts: &RequestParts<'_>) -> Result<Self, Self::Rejection> {
        let state = parts
            .context
            .app_state::<T>()
            .ok_or_else(|| ExtractError::internal("application state is not available"))?;
        Ok(Self(state))
    }
}

/// `Json<T>` extractor
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Json<T>(pub T);

impl<T> Deref for Json<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[async_trait]
impl<T> FromRequest for Json<T>
where
    T: DeserializeOwned + Send,
{
    type Rejection = ExtractError;

    async fn from_request(req: &Request) -> Result<Self, Self::Rejection> {
        let content_type = req
            .headers
            .get("content-type")
            .ok_or_else(|| ExtractError::bad_request("missing Content-Type header"))?;

        if !is_json_like_content_type(content_type) {
            return Err(ExtractError::bad_request(format!(
                "unsupported Content-Type: {}",
                content_type
            )));
        }

        let body = req
            .body
            .as_ref()
            .ok_or_else(|| ExtractError::bad_request("missing request body"))?;

        if body.is_empty() {
            return Err(ExtractError::bad_request("missing request body"));
        }

        let value = serde_json::from_slice(body)
            .map_err(|err| ExtractError::bad_request(err.to_string()))?;
        Ok(Self(value))
    }
}
