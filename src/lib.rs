//! RunBridge: AWS LambdaとGoogle Cloud Run向けの統一サーバレスAPIフレームワーク
//!
//! 単一のコードベースで異なるサーバレス環境に対応するためのライブラリ

#[cfg(all(
    not(feature = "allow_feature_conflicts"),
    feature = "lambda",
    feature = "cloud_run"
))]
compile_error!(
    "Conflicting features: 'lambda' and 'cloud_run' cannot be enabled together. Choose exactly one."
);

#[cfg(all(
    not(feature = "allow_feature_conflicts"),
    feature = "lambda",
    feature = "cgi"
))]
compile_error!(
    "Conflicting features: 'lambda' and 'cgi' cannot be enabled together. Choose exactly one."
);

#[cfg(all(
    not(feature = "allow_feature_conflicts"),
    feature = "cloud_run",
    feature = "cgi"
))]
compile_error!(
    "Conflicting features: 'cloud_run' and 'cgi' cannot be enabled together. Choose exactly one."
);

#[cfg(all(
    not(feature = "lambda"),
    not(feature = "cloud_run"),
    not(feature = "cgi")
))]
#[deprecated(note = "No target feature enabled. Enable one of: 'lambda', 'cloud_run', or 'cgi'.")]
pub const _RUNBRIDGE_NO_TARGET_FEATURE_WARNING: () = ();

#[cfg(all(
    not(feature = "lambda"),
    not(feature = "cloud_run"),
    not(feature = "cgi")
))]
const _: () = {
    let _ = _RUNBRIDGE_NO_TARGET_FEATURE_WARNING;
};

pub mod common;
pub mod error;
pub mod handler;

#[cfg(feature = "lambda")]
pub mod lambda;

#[cfg(feature = "cloud_run")]
pub mod cloudrun;

#[cfg(feature = "cgi")]
pub mod cgi;

pub use common::*;
pub use error::*;
pub use handler::*;

use async_trait::async_trait;
use std::any::Any;
use std::collections::HashSet;
use std::sync::Arc;

/// リクエストを処理するアプリケーションを構築するためのビルダー
pub struct RunBridgeBuilder {
    routes: Vec<handler::Route>,
    middlewares: Vec<Arc<dyn common::Middleware>>,
    fallback: Option<Arc<dyn common::Handler>>,
    state: Option<Arc<dyn Any + Send + Sync>>,
}

impl Default for RunBridgeBuilder {
    fn default() -> Self {
        Self {
            routes: Vec::new(),
            middlewares: Vec::new(),
            fallback: None,
            state: None,
        }
    }
}

impl RunBridgeBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn handler(mut self, route: handler::Route) -> Self {
        if self
            .routes
            .iter()
            .any(|existing| existing.method() == route.method() && existing.path() == route.path())
        {
            log::warn!(
                "Duplicate route registration ignored: {} {}",
                route.method(),
                route.path()
            );
            return self;
        }

        self.routes.push(route);
        self
    }

    pub fn middleware<M>(mut self, middleware: M) -> Self
    where
        M: common::Middleware + 'static,
    {
        self.middlewares.push(Arc::new(middleware));
        self
    }

    pub fn fallback<H>(mut self, handler: H) -> Self
    where
        H: common::Handler + 'static,
    {
        self.fallback = Some(Arc::new(handler));
        self
    }

    pub fn state<T>(mut self, state: Arc<T>) -> Self
    where
        T: Send + Sync + 'static,
    {
        self.state = Some(state);
        self
    }

    pub fn build(self) -> RunBridge {
        RunBridge {
            routes: self.routes,
            middlewares: self.middlewares,
            fallback: self.fallback,
            state: self.state,
        }
    }
}

/// リクエストを処理するアプリケーション
pub struct RunBridge {
    routes: Vec<handler::Route>,
    middlewares: Vec<Arc<dyn common::Middleware>>,
    fallback: Option<Arc<dyn common::Handler>>,
    state: Option<Arc<dyn Any + Send + Sync>>,
}

#[derive(Clone)]
struct StaticResponseHandler {
    response: common::Response,
}

#[async_trait]
impl common::Handler for StaticResponseHandler {
    async fn handle(&self, _req: common::Request) -> Result<common::Response, error::Error> {
        Ok(self.response.clone())
    }
}

impl RunBridge {
    pub fn builder() -> RunBridgeBuilder {
        RunBridgeBuilder::new()
    }

    pub fn resolve(&self, path: &str, method: &common::Method) -> handler::RouteMatch<'_> {
        let mut best_score: Option<(usize, usize)> = None;
        let mut candidates: Vec<(&handler::Route, std::collections::HashMap<String, String>)> =
            Vec::new();

        for route in &self.routes {
            let Some(path_params) = route.match_path(path) else {
                continue;
            };
            let score = route.path_score();
            match best_score {
                None => {
                    best_score = Some(score);
                    candidates.push((route, path_params));
                }
                Some(current) if score > current => {
                    best_score = Some(score);
                    candidates.clear();
                    candidates.push((route, path_params));
                }
                Some(current) if score == current => {
                    candidates.push((route, path_params));
                }
                Some(_) => {}
            }
        }

        if candidates.is_empty() {
            return handler::RouteMatch::NotFound;
        }

        for (route, path_params) in &candidates {
            if route.method() == *method {
                return handler::RouteMatch::Matched {
                    route,
                    path_params: path_params.clone(),
                };
            }
        }

        let mut seen = HashSet::new();
        let mut allow = Vec::new();
        for candidate_method in common::Method::ALL {
            for (route, _) in &candidates {
                if route.method() == candidate_method && seen.insert(candidate_method) {
                    allow.push(candidate_method);
                }
            }
        }

        handler::RouteMatch::MethodNotAllowed { allow }
    }

    pub async fn handle_request(&self, mut request: common::Request) -> common::Response {
        request.path_params.clear();
        request
            .context_mut()
            .set_app_state(self.state.as_ref().map(Arc::clone));

        let static_handler;
        let endpoint: &dyn common::Handler = match self.resolve(&request.path, &request.method) {
            handler::RouteMatch::Matched { route, path_params } => {
                request.path_params = path_params;
                route.handler().as_ref()
            }
            handler::RouteMatch::MethodNotAllowed { allow } => {
                let allow_header = allow
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ");
                static_handler = StaticResponseHandler {
                    response: common::Response::method_not_allowed()
                        .with_header("Content-Type", "text/plain")
                        .with_header("Allow", allow_header)
                        .with_body("Method Not Allowed".as_bytes().to_vec()),
                };
                &static_handler
            }
            handler::RouteMatch::NotFound => {
                if let Some(fallback) = &self.fallback {
                    fallback.as_ref()
                } else {
                    static_handler = StaticResponseHandler {
                        response: common::Response::not_found()
                            .with_header("Content-Type", "text/plain")
                            .with_body("Not Found".as_bytes().to_vec()),
                    };
                    &static_handler
                }
            }
        };

        let next = common::Next {
            middlewares: &self.middlewares,
            endpoint,
        };

        match next.run(request).await {
            Ok(response) => response,
            Err(error) => common::Response::from_error(&error),
        }
    }

    pub fn middlewares(&self) -> &[Arc<dyn common::Middleware>] {
        &self.middlewares
    }
}
