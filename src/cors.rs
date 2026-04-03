use async_trait::async_trait;

use crate::common::{Method, Middleware, Next, Request, Response};
use crate::error::Error;

#[derive(Clone, Default)]
pub struct Cors {
    allow_any_origin: bool,
    allow_origins: Vec<String>,
    allow_methods: Vec<Method>,
    allow_headers: Vec<String>,
    expose_headers: Vec<String>,
    allow_credentials: bool,
    max_age: Option<u32>,
}

impl Cors {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn allow_origin(mut self, origin: impl Into<String>) -> Self {
        self.allow_origins.push(origin.into());
        self
    }

    pub fn allow_any_origin(mut self) -> Self {
        self.allow_any_origin = true;
        self.allow_origins.clear();
        self
    }

    pub fn allow_method(mut self, method: Method) -> Self {
        if !self.allow_methods.contains(&method) {
            self.allow_methods.push(method);
        }
        self
    }

    pub fn allow_methods<I>(mut self, methods: I) -> Self
    where
        I: IntoIterator<Item = Method>,
    {
        for method in methods {
            self = self.allow_method(method);
        }
        self
    }

    pub fn allow_header(mut self, header: impl Into<String>) -> Self {
        self.allow_headers.push(header.into());
        self
    }

    pub fn allow_headers<I, S>(mut self, headers: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        for header in headers {
            self = self.allow_header(header);
        }
        self
    }

    pub fn expose_header(mut self, header: impl Into<String>) -> Self {
        self.expose_headers.push(header.into());
        self
    }

    pub fn expose_headers<I, S>(mut self, headers: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        for header in headers {
            self = self.expose_header(header);
        }
        self
    }

    pub fn allow_credentials(mut self, yes: bool) -> Self {
        self.allow_credentials = yes;
        self
    }

    pub fn max_age(mut self, seconds: u32) -> Self {
        self.max_age = Some(seconds);
        self
    }

    pub fn try_validate(&self) -> Result<(), Error> {
        if self.allow_any_origin && self.allow_credentials {
            return Err(Error::ConfigurationError(
                "CORS cannot combine allow_any_origin() with allow_credentials(true)".to_string(),
            ));
        }

        Ok(())
    }

    fn is_preflight(req: &Request) -> bool {
        req.method == Method::OPTIONS
            && req.headers.get("origin").is_some()
            && req.headers.get("access-control-request-method").is_some()
    }

    fn allowed_origin_value(&self, origin: &str) -> Option<String> {
        if self.allow_any_origin {
            return Some("*".to_string());
        }

        self.allow_origins
            .iter()
            .any(|allowed| allowed == origin)
            .then(|| origin.to_string())
    }

    fn requested_headers_allowed(&self, req: &Request) -> bool {
        let Some(requested_headers) = req.headers.get("access-control-request-headers") else {
            return true;
        };

        if self.allow_headers.is_empty() {
            return false;
        }

        requested_headers.split(',').all(|header| {
            let header = header.trim();
            self.allow_headers
                .iter()
                .any(|allowed| allowed.eq_ignore_ascii_case(header))
        })
    }

    fn requested_method_allowed(&self, req: &Request) -> bool {
        let Some(method) = req.headers.get("access-control-request-method") else {
            return false;
        };

        self.allow_methods
            .iter()
            .any(|allowed| allowed.to_string().eq_ignore_ascii_case(method))
    }
}

#[async_trait]
impl Middleware for Cors {
    async fn handle(&self, req: Request, next: Next<'_>) -> Result<Response, Error> {
        self.try_validate()?;

        if Self::is_preflight(&req) {
            let mut response = Response::no_content();
            let origin = req.headers.get("origin").unwrap_or_default();

            if let Some(allow_origin) = self.allowed_origin_value(origin) {
                if self.requested_method_allowed(&req) && self.requested_headers_allowed(&req) {
                    response = response.with_header("Access-Control-Allow-Origin", allow_origin);
                    response = response.with_header(
                        "Access-Control-Allow-Methods",
                        self.allow_methods
                            .iter()
                            .map(ToString::to_string)
                            .collect::<Vec<_>>()
                            .join(", "),
                    );

                    if !self.allow_headers.is_empty() {
                        response = response.with_header(
                            "Access-Control-Allow-Headers",
                            self.allow_headers.join(", "),
                        );
                    }

                    if self.allow_credentials {
                        response = response.with_header("Access-Control-Allow-Credentials", "true");
                    }

                    if let Some(max_age) = self.max_age {
                        response =
                            response.with_header("Access-Control-Max-Age", max_age.to_string());
                    }

                    response = append_vary(response, "Origin");
                    response = append_vary(response, "Access-Control-Request-Method");
                    if req.headers.get("access-control-request-headers").is_some() {
                        response = append_vary(response, "Access-Control-Request-Headers");
                    }
                }
            }

            return Ok(response);
        }

        let origin = req.headers.get("origin").map(str::to_string);
        let mut response = next.run(req).await?;

        if let Some(origin) = origin {
            if let Some(allow_origin) = self.allowed_origin_value(&origin) {
                response = response.with_header("Access-Control-Allow-Origin", allow_origin);
                if self.allow_credentials {
                    response = response.with_header("Access-Control-Allow-Credentials", "true");
                }
                if !self.expose_headers.is_empty() {
                    response = response.with_header(
                        "Access-Control-Expose-Headers",
                        self.expose_headers.join(", "),
                    );
                }
                response = append_vary(response, "Origin");
            }
        }

        Ok(response)
    }
}

fn append_vary(mut response: Response, value: &str) -> Response {
    let already_present = response.headers.get_all("vary").iter().any(|existing| {
        existing
            .split(',')
            .any(|part| part.trim().eq_ignore_ascii_case(value))
    });

    if !already_present {
        response.headers.append("Vary", value);
    }

    response
}
