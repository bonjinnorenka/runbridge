use std::sync::Arc;

use crate::common::Middleware;
use crate::error::Error;
use crate::handler::Route;

pub(crate) struct RouterScope {
    prefix: String,
    middlewares: Vec<Arc<dyn Middleware>>,
}

impl RouterScope {
    fn root() -> Self {
        Self {
            prefix: "/".to_string(),
            middlewares: Vec::new(),
        }
    }

    pub(crate) fn matches(&self, path: &str) -> bool {
        scope_matches(&self.prefix, path)
    }

    pub(crate) fn depth(&self) -> usize {
        if self.prefix == "/" {
            0
        } else {
            self.prefix
                .split('/')
                .filter(|segment| !segment.is_empty())
                .count()
        }
    }

    pub(crate) fn middlewares(&self) -> &[Arc<dyn Middleware>] {
        &self.middlewares
    }

    fn push_middleware(&mut self, middleware: Arc<dyn Middleware>) {
        self.middlewares.push(middleware);
    }

    fn try_with_prefix(mut self, prefix: &str) -> Result<Self, Error> {
        let joined = join_paths(prefix, &self.prefix)?;
        self.prefix = canonicalize_scope_prefix(&joined);
        Ok(self)
    }
}

pub struct Router {
    routes: Vec<Route>,
    middlewares: Vec<Arc<dyn Middleware>>,
    scopes: Vec<RouterScope>,
}

impl Default for Router {
    fn default() -> Self {
        Self {
            routes: Vec::new(),
            middlewares: Vec::new(),
            scopes: vec![RouterScope::root()],
        }
    }
}

impl Router {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn route(mut self, mut route: Route) -> Self {
        route.prepend_group_middlewares(&self.middlewares);
        self.routes.push(route);
        self
    }

    pub fn middleware<M>(mut self, middleware: M) -> Self
    where
        M: Middleware + 'static,
    {
        let middleware = Arc::new(middleware) as Arc<dyn Middleware>;
        for route in &mut self.routes {
            route.push_group_middleware(Arc::clone(&middleware));
        }
        self.middlewares.push(middleware);
        self.scopes[0].push_middleware(Arc::clone(self.middlewares.last().unwrap()));
        self
    }

    pub fn merge(mut self, mut other: Router) -> Self {
        for route in &mut other.routes {
            route.prepend_group_middlewares(&self.middlewares);
        }
        self.routes.extend(other.routes);
        self.scopes.extend(other.scopes);
        self
    }

    pub fn nest(self, prefix: impl Into<String>, other: Router) -> Self {
        let prefix = prefix.into();
        self.try_nest(prefix, other)
            .unwrap_or_else(|err| panic!("Failed to nest router: {}", err))
    }

    pub(crate) fn try_nest(mut self, prefix: String, mut other: Router) -> Result<Self, Error> {
        normalize_prefix(&prefix)?;

        for route in &mut other.routes {
            route.prepend_group_middlewares(&self.middlewares);
        }

        for route in other.routes {
            self.routes.push(route.try_with_path_prefix(&prefix)?);
        }

        for scope in other.scopes {
            self.scopes.push(scope.try_with_prefix(&prefix)?);
        }

        Ok(self)
    }

    pub(crate) fn into_parts(self) -> (Vec<Route>, Vec<RouterScope>) {
        (self.routes, self.scopes)
    }
}

fn normalize_prefix(prefix: &str) -> Result<(), Error> {
    if !prefix.starts_with('/') {
        return Err(Error::ConfigurationError(format!(
            "router prefix must start with '/': {}",
            prefix
        )));
    }

    if prefix != "/" && prefix.contains("//") {
        return Err(Error::ConfigurationError(format!(
            "router prefix must not contain empty segments: {}",
            prefix
        )));
    }

    Ok(())
}

pub(crate) fn join_paths(prefix: &str, path: &str) -> Result<String, Error> {
    normalize_prefix(prefix)?;

    if !path.starts_with('/') {
        return Err(Error::ConfigurationError(format!(
            "route template must start with '/': {}",
            path
        )));
    }

    if prefix == "/" {
        return Ok(path.to_string());
    }

    if path == "/" {
        return Ok(prefix.to_string());
    }

    let prefix = prefix.trim_end_matches('/');
    Ok(format!("{}{}", prefix, path))
}

fn canonicalize_scope_prefix(prefix: &str) -> String {
    if prefix == "/" {
        "/".to_string()
    } else {
        prefix.trim_end_matches('/').to_string()
    }
}

fn scope_matches(prefix: &str, path: &str) -> bool {
    if prefix == "/" {
        return true;
    }

    path == prefix || path.starts_with(&format!("{}/", prefix))
}
