use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;

use crate::common::utils::percent_decode_path_segment;
use crate::common::{Handler, Method, Request, Response};
use crate::error::Error;

type HandlerFuture = Pin<Box<dyn Future<Output = Result<Response, Error>> + Send + 'static>>;
type HandlerFn = dyn Fn(Request) -> HandlerFuture + Send + Sync + 'static;

#[derive(Clone)]
struct BoxedHandler {
    inner: Arc<HandlerFn>,
}

impl BoxedHandler {
    fn new<F>(handler: F) -> Self
    where
        F: Fn(Request) -> HandlerFuture + Send + Sync + 'static,
    {
        Self {
            inner: Arc::new(handler),
        }
    }
}

#[async_trait]
impl Handler for BoxedHandler {
    async fn handle(&self, req: Request) -> Result<Response, Error> {
        (self.inner)(req).await
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PathSegment {
    Static(String),
    Param(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PathPattern {
    segments: Vec<PathSegment>,
    trailing_slash: bool,
}

impl PathPattern {
    fn parse(template: &str) -> Result<Self, Error> {
        if !template.starts_with('/') {
            return Err(Error::ConfigurationError(format!(
                "route template must start with '/': {}",
                template
            )));
        }

        if template != "/" && template.contains("//") {
            return Err(Error::ConfigurationError(format!(
                "route template must not contain empty segments: {}",
                template
            )));
        }

        let trailing_slash = template.len() > 1 && template.ends_with('/');
        let body = if template == "/" {
            ""
        } else if trailing_slash {
            &template[1..template.len() - 1]
        } else {
            &template[1..]
        };

        let mut segments = Vec::new();
        if !body.is_empty() {
            for segment in body.split('/') {
                if segment.is_empty() {
                    return Err(Error::ConfigurationError(format!(
                        "route template must not contain empty segments: {}",
                        template
                    )));
                }

                if segment.starts_with('{') || segment.ends_with('}') {
                    if !(segment.starts_with('{') && segment.ends_with('}')) {
                        return Err(Error::ConfigurationError(format!(
                            "invalid path parameter segment: {}",
                            segment
                        )));
                    }
                    let name = &segment[1..segment.len() - 1];
                    if name.is_empty()
                        || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                    {
                        return Err(Error::ConfigurationError(format!(
                            "invalid path parameter name: {}",
                            name
                        )));
                    }
                    segments.push(PathSegment::Param(name.to_string()));
                    continue;
                }

                if segment.contains('{') || segment.contains('}') {
                    return Err(Error::ConfigurationError(format!(
                        "invalid path template segment: {}",
                        segment
                    )));
                }

                if segment.chars().any(|c| {
                    matches!(
                        c,
                        '^' | '$' | '+' | '*' | '?' | '[' | ']' | '(' | ')' | '|' | '\\'
                    )
                }) {
                    return Err(Error::ConfigurationError(format!(
                        "regex-style route templates are not supported: {}",
                        template
                    )));
                }

                segments.push(PathSegment::Static(segment.to_string()));
            }
        }

        Ok(Self {
            segments,
            trailing_slash,
        })
    }

    fn match_path(&self, path: &str) -> Option<HashMap<String, String>> {
        if !path.starts_with('/') {
            return None;
        }

        let trailing_slash = path.len() > 1 && path.ends_with('/');
        if self.trailing_slash != trailing_slash {
            return None;
        }

        let body = if path == "/" {
            ""
        } else if trailing_slash {
            &path[1..path.len() - 1]
        } else {
            &path[1..]
        };

        let path_segments: Vec<&str> = if body.is_empty() {
            Vec::new()
        } else {
            body.split('/').collect()
        };

        if path_segments.len() != self.segments.len() {
            return None;
        }

        let mut params = HashMap::new();

        for (template_segment, actual_segment) in self.segments.iter().zip(path_segments) {
            match template_segment {
                PathSegment::Static(expected) => {
                    if expected != actual_segment {
                        return None;
                    }
                }
                PathSegment::Param(name) => {
                    params.insert(name.clone(), percent_decode_path_segment(actual_segment));
                }
            }
        }

        Some(params)
    }

    fn static_segment_count(&self) -> usize {
        self.segments
            .iter()
            .filter(|segment| matches!(segment, PathSegment::Static(_)))
            .count()
    }

    fn segment_count(&self) -> usize {
        self.segments.len()
    }
}

/// 登録済みルート
pub struct Route {
    method: Method,
    path: String,
    pattern: PathPattern,
    handler: Arc<dyn Handler>,
}

impl Route {
    pub fn new<H>(method: Method, path: impl Into<String>, handler: H) -> Result<Self, Error>
    where
        H: Handler + 'static,
    {
        let path = path.into();
        let pattern = PathPattern::parse(&path)?;
        Ok(Self {
            method,
            path,
            pattern,
            handler: Arc::new(handler),
        })
    }

    pub fn method(&self) -> Method {
        self.method
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn matches(&self, path: &str, method: &Method) -> bool {
        self.method == *method && self.pattern.match_path(path).is_some()
    }

    pub(crate) fn handler(&self) -> &Arc<dyn Handler> {
        &self.handler
    }

    pub(crate) fn match_path(&self, path: &str) -> Option<HashMap<String, String>> {
        self.pattern.match_path(path)
    }

    pub(crate) fn path_score(&self) -> (usize, usize) {
        (
            self.pattern.static_segment_count(),
            self.pattern.segment_count(),
        )
    }
}

#[async_trait]
impl Handler for Route {
    async fn handle(&self, req: Request) -> Result<Response, Error> {
        self.handler.handle(req).await
    }
}

/// ルーティング結果
pub enum RouteMatch<'a> {
    Matched {
        route: &'a Route,
        path_params: HashMap<String, String>,
    },
    MethodNotAllowed {
        allow: Vec<Method>,
    },
    NotFound,
}

pub fn try_route<H>(method: Method, path: impl Into<String>, handler: H) -> Result<Route, Error>
where
    H: Handler + 'static,
{
    Route::new(method, path, handler)
}

pub fn route<H>(method: Method, path: impl Into<String>, handler: H) -> Route
where
    H: Handler + 'static,
{
    try_route(method, path, handler).unwrap_or_else(|err| panic!("Failed to create route: {}", err))
}

pub(crate) fn sync_handler<F, R>(handler: F) -> impl Handler
where
    F: Fn(Request) -> Result<R, Error> + Send + Sync + 'static,
    R: super::IntoResponse + 'static,
{
    BoxedHandler::new(move |req| {
        let result = handler(req).map(|value| value.into_response());
        Box::pin(async move { result })
    })
}

pub(crate) fn async_handler<F, Fut, R>(handler: F) -> impl Handler
where
    F: Fn(Request) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<R, Error>> + Send + 'static,
    R: super::IntoResponse + 'static,
{
    BoxedHandler::new(move |req| {
        let fut = handler(req);
        Box::pin(async move { fut.await.map(|value| value.into_response()) })
    })
}
