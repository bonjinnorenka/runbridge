//! HTTP関連の基本型とユーティリティ

use super::context::RequestContext;
use super::cookie::Cookie;
use super::utils::{get_max_body_size, is_header_value_valid};
use crate::error::Error;
use bytes::Bytes;
use flate2::read::GzDecoder;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::io::Read;

/// HTTPステータスコード
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusCode {
    Ok = 200,
    Created = 201,
    NoContent = 204,
    BadRequest = 400,
    Unauthorized = 401,
    Forbidden = 403,
    NotFound = 404,
    MethodNotAllowed = 405,
    Conflict = 409,
    UnprocessableEntity = 422,
    Locked = 423,
    TooManyRequests = 429,
    InternalServerError = 500,
    NotImplemented = 501,
    BadGateway = 502,
    ServiceUnavailable = 503,
}

impl StatusCode {
    pub fn as_u16(&self) -> u16 {
        *self as u16
    }

    pub fn reason_phrase(&self) -> &'static str {
        match self {
            StatusCode::Ok => "OK",
            StatusCode::Created => "Created",
            StatusCode::NoContent => "No Content",
            StatusCode::BadRequest => "Bad Request",
            StatusCode::Unauthorized => "Unauthorized",
            StatusCode::Forbidden => "Forbidden",
            StatusCode::NotFound => "Not Found",
            StatusCode::MethodNotAllowed => "Method Not Allowed",
            StatusCode::Conflict => "Conflict",
            StatusCode::UnprocessableEntity => "Unprocessable Entity",
            StatusCode::Locked => "Locked",
            StatusCode::TooManyRequests => "Too Many Requests",
            StatusCode::InternalServerError => "Internal Server Error",
            StatusCode::NotImplemented => "Not Implemented",
            StatusCode::BadGateway => "Bad Gateway",
            StatusCode::ServiceUnavailable => "Service Unavailable",
        }
    }

    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.as_u16())
    }

    pub fn is_client_error(&self) -> bool {
        (400..500).contains(&self.as_u16())
    }

    pub fn is_server_error(&self) -> bool {
        (500..600).contains(&self.as_u16())
    }
}

impl From<StatusCode> for u16 {
    fn from(status: StatusCode) -> u16 {
        status.as_u16()
    }
}

/// HTTPメソッド
#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub enum Method {
    GET,
    POST,
    PUT,
    DELETE,
    PATCH,
    HEAD,
    OPTIONS,
}

impl Method {
    pub const ALL: [Method; 7] = [
        Method::GET,
        Method::POST,
        Method::PUT,
        Method::DELETE,
        Method::PATCH,
        Method::HEAD,
        Method::OPTIONS,
    ];

    pub fn from_str(method: &str) -> Option<Self> {
        match method.to_uppercase().as_str() {
            "GET" => Some(Method::GET),
            "POST" => Some(Method::POST),
            "PUT" => Some(Method::PUT),
            "DELETE" => Some(Method::DELETE),
            "PATCH" => Some(Method::PATCH),
            "HEAD" => Some(Method::HEAD),
            "OPTIONS" => Some(Method::OPTIONS),
            _ => None,
        }
    }
}

impl fmt::Display for Method {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Method::GET => write!(f, "GET"),
            Method::POST => write!(f, "POST"),
            Method::PUT => write!(f, "PUT"),
            Method::DELETE => write!(f, "DELETE"),
            Method::PATCH => write!(f, "PATCH"),
            Method::HEAD => write!(f, "HEAD"),
            Method::OPTIONS => write!(f, "OPTIONS"),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct MultiMap {
    entries: Vec<(String, String)>,
}

impl MultiMap {
    fn new() -> Self {
        Self { entries: Vec::new() }
    }

    fn insert_case_sensitive(&mut self, key: impl Into<String>, value: impl Into<String>) {
        let key = key.into();
        self.entries.retain(|(existing, _)| existing != &key);
        self.entries.push((key, value.into()));
    }

    fn append_case_sensitive(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.entries.push((key.into(), value.into()));
    }

    fn get_case_sensitive(&self, key: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|(existing, _)| existing == key)
            .map(|(_, value)| value.as_str())
    }

    fn get_all_case_sensitive(&self, key: &str) -> Vec<&str> {
        self.entries
            .iter()
            .filter(|(existing, _)| existing == key)
            .map(|(_, value)| value.as_str())
            .collect()
    }

    fn remove_case_sensitive(&mut self, key: &str) -> Vec<String> {
        let mut removed = Vec::new();
        self.entries.retain(|(existing, value)| {
            if existing == key {
                removed.push(value.clone());
                false
            } else {
                true
            }
        });
        removed
    }

    fn contains_case_sensitive(&self, key: &str) -> bool {
        self.entries.iter().any(|(existing, _)| existing == key)
    }

    fn insert_case_insensitive(&mut self, key: impl Into<String>, value: impl Into<String>) {
        let key = key.into();
        self.entries
            .retain(|(existing, _)| !existing.eq_ignore_ascii_case(&key));
        self.entries.push((key, value.into()));
    }

    fn append_case_insensitive(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.entries.push((key.into(), value.into()));
    }

    fn get_case_insensitive(&self, key: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|(existing, _)| existing.eq_ignore_ascii_case(key))
            .map(|(_, value)| value.as_str())
    }

    fn get_all_case_insensitive(&self, key: &str) -> Vec<&str> {
        self.entries
            .iter()
            .filter(|(existing, _)| existing.eq_ignore_ascii_case(key))
            .map(|(_, value)| value.as_str())
            .collect()
    }

    fn remove_case_insensitive(&mut self, key: &str) -> Vec<String> {
        let mut removed = Vec::new();
        self.entries.retain(|(existing, value)| {
            if existing.eq_ignore_ascii_case(key) {
                removed.push(value.clone());
                false
            } else {
                true
            }
        });
        removed
    }

    fn contains_case_insensitive(&self, key: &str) -> bool {
        self.entries
            .iter()
            .any(|(existing, _)| existing.eq_ignore_ascii_case(key))
    }

    fn len(&self) -> usize {
        self.entries.len()
    }

    fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn iter(&self) -> MultiMapIter<'_> {
        MultiMapIter {
            inner: self.entries.iter(),
        }
    }
}

pub struct MultiMapIter<'a> {
    inner: std::slice::Iter<'a, (String, String)>,
}

impl<'a> Iterator for MultiMapIter<'a> {
    type Item = (&'a str, &'a str);

    fn next(&mut self) -> Option<Self::Item> {
        self.inner
            .next()
            .map(|(key, value)| (key.as_str(), value.as_str()))
    }
}

/// 大文字小文字非依存・複数値対応のHTTPヘッダーマップ
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HeaderMap {
    inner: MultiMap,
}

impl HeaderMap {
    pub fn new() -> Self {
        Self {
            inner: MultiMap::new(),
        }
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.inner.get_case_insensitive(key)
    }

    pub fn get_all(&self, key: &str) -> Vec<&str> {
        self.inner.get_all_case_insensitive(key)
    }

    pub fn insert(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.inner.insert_case_insensitive(key, value);
    }

    pub fn append(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.inner.append_case_insensitive(key, value);
    }

    pub fn remove(&mut self, key: &str) -> Vec<String> {
        self.inner.remove_case_insensitive(key)
    }

    pub fn contains_key(&self, key: &str) -> bool {
        self.inner.contains_case_insensitive(key)
    }

    pub fn iter_all(&self) -> MultiMapIter<'_> {
        self.inner.iter()
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

impl<'a> IntoIterator for &'a HeaderMap {
    type Item = (&'a str, &'a str);
    type IntoIter = MultiMapIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter_all()
    }
}

/// 複数値対応のクエリマップ
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QueryMap {
    inner: MultiMap,
}

impl QueryMap {
    pub fn new() -> Self {
        Self {
            inner: MultiMap::new(),
        }
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.inner.get_case_sensitive(key)
    }

    pub fn get_all(&self, key: &str) -> Vec<&str> {
        self.inner.get_all_case_sensitive(key)
    }

    pub fn append(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.inner.append_case_sensitive(key, value);
    }

    pub fn insert(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.inner.insert_case_sensitive(key, value);
    }

    pub fn remove(&mut self, key: &str) -> Vec<String> {
        self.inner.remove_case_sensitive(key)
    }

    pub fn contains_key(&self, key: &str) -> bool {
        self.inner.contains_case_sensitive(key)
    }

    pub fn iter(&self) -> MultiMapIter<'_> {
        self.inner.iter()
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    pub fn to_owned_pairs(&self) -> Vec<(String, String)> {
        self.inner.entries.clone()
    }
}

impl<'a> IntoIterator for &'a QueryMap {
    type Item = (&'a str, &'a str);
    type IntoIter = MultiMapIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// HTTPリクエスト
#[derive(Debug)]
pub struct Request {
    pub method: Method,
    pub path: String,
    pub query: QueryMap,
    pub headers: HeaderMap,
    pub path_params: HashMap<String, String>,
    pub cookies: Vec<Cookie>,
    pub body: Option<Bytes>,
    context: RequestContext,
}

impl Request {
    pub fn new(method: Method, path: String) -> Self {
        Self {
            method,
            path,
            query: QueryMap::new(),
            headers: HeaderMap::new(),
            path_params: HashMap::new(),
            cookies: Vec::new(),
            body: None,
            context: RequestContext::new(),
        }
    }

    pub fn with_query_param(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.query.append(key, value);
        self
    }

    pub fn with_header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        let key = key.into();
        let value = value.into();
        if !is_header_value_valid(&value) {
            log::warn!(
                "Request::with_header rejected invalid value for '{}': {:?}",
                key,
                value
            );
            return self;
        }
        self.headers.append(key, value);
        self
    }

    pub fn with_path_param(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.path_params.insert(key.into(), value.into());
        self
    }

    pub fn with_cookie(mut self, cookie: Cookie) -> Self {
        self.cookies.push(cookie);
        self
    }

    pub fn with_body(mut self, body: impl Into<Bytes>) -> Self {
        self.body = Some(body.into());
        self
    }

    pub fn query_param(&self, key: &str) -> Option<&str> {
        self.query.get(key)
    }

    pub fn header(&self, key: &str) -> Option<&str> {
        self.headers.get(key)
    }

    pub fn json<T: for<'de> Deserialize<'de>>(&self) -> Result<T, Error> {
        if let Some(body) = &self.body {
            serde_json::from_slice(body).map_err(|e| Error::InvalidRequestBody(e.to_string()))
        } else {
            Err(Error::InvalidRequestBody("No request body".to_string()))
        }
    }

    pub fn context(&self) -> &RequestContext {
        &self.context
    }

    pub fn context_mut(&mut self) -> &mut RequestContext {
        &mut self.context
    }

    pub fn with_context(mut self, context: RequestContext) -> Self {
        self.context = context;
        self
    }

    pub fn clone_without_context(&self) -> Self {
        #[cfg(debug_assertions)]
        log::debug!("Request::clone_without_context() called - metadata context will be empty");

        Self {
            method: self.method,
            path: self.path.clone(),
            query: self.query.clone(),
            headers: self.headers.clone(),
            path_params: self.path_params.clone(),
            cookies: self.cookies.clone(),
            body: self.body.clone(),
            context: self.context.clone_empty(),
        }
    }

    /// リクエストボディがgzipエンコードされている場合は解凍する
    pub fn decompress_gzip_body(&mut self) -> Result<(), Error> {
        if let Some(encoding) = self.headers.get("content-encoding") {
            if encoding.eq_ignore_ascii_case("gzip") {
                if let Some(body_data) = &self.body {
                    let max_body_size = get_max_body_size();
                    let mut decoder = GzDecoder::new(body_data.as_ref());
                    let mut decompressed = Vec::new();
                    let mut buffer = [0u8; 8192];

                    loop {
                        match decoder.read(&mut buffer) {
                            Ok(0) => break,
                            Ok(n) => {
                                if decompressed.len() + n > max_body_size {
                                    return Err(Error::PayloadTooLarge(format!(
                                        "Decompressed body too large (>{} bytes)",
                                        max_body_size
                                    )));
                                }
                                decompressed.extend_from_slice(&buffer[..n]);
                            }
                            Err(err) => {
                                return Err(Error::InvalidRequestBody(format!(
                                    "Invalid gzip-encoded request body: {}",
                                    err
                                )));
                            }
                        }
                    }

                    self.body = Some(Bytes::from(decompressed));
                    self.headers.remove("content-encoding");
                }
            }
        }
        Ok(())
    }
}

/// HTTPレスポンス
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    pub status: u16,
    pub headers: HeaderMap,
    pub cookies: Vec<Cookie>,
    pub body: Option<Bytes>,
}

impl Response {
    pub fn new(status: u16) -> Self {
        let mut headers = HeaderMap::new();
        inject_default_security_headers(&mut headers);
        Self {
            status,
            headers,
            cookies: Vec::new(),
            body: None,
        }
    }

    pub fn with_status(status: StatusCode) -> Self {
        Self::new(status.as_u16())
    }

    pub fn with_header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        let key = key.into();
        let value = value.into();
        if !is_header_value_valid(&value) {
            log::warn!(
                "Response::with_header rejected invalid value for '{}': {:?}",
                key,
                value
            );
            return self;
        }
        self.headers.insert(key, value);
        self
    }

    pub fn append_header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        let key = key.into();
        let value = value.into();
        if !is_header_value_valid(&value) {
            log::warn!(
                "Response::append_header rejected invalid value for '{}': {:?}",
                key,
                value
            );
            return self;
        }
        self.headers.append(key, value);
        self
    }

    pub fn with_cookie(mut self, cookie: Cookie) -> Self {
        self.cookies.push(cookie);
        self
    }

    pub fn with_body(mut self, body: impl Into<Bytes>) -> Self {
        self.body = Some(body.into());
        self
    }

    pub fn json<T: Serialize>(mut self, value: &T) -> Result<Self, Error> {
        let json = serde_json::to_vec(value)
            .map_err(|e| Error::ResponseSerializationError(e.to_string()))?;
        self.headers.insert("Content-Type", "application/json");
        self.body = Some(Bytes::from(json));
        Ok(self)
    }

    pub fn ok() -> Self {
        Self::new(200)
    }

    pub fn created() -> Self {
        Self::new(201)
    }

    pub fn no_content() -> Self {
        Self::new(204)
    }

    pub fn bad_request() -> Self {
        Self::new(400)
    }

    pub fn unauthorized() -> Self {
        Self::new(401)
    }

    pub fn forbidden() -> Self {
        Self::new(403)
    }

    pub fn not_found() -> Self {
        Self::new(404)
    }

    pub fn method_not_allowed() -> Self {
        Self::new(405)
    }

    pub fn internal_server_error() -> Self {
        Self::new(500)
    }

    pub fn from_error(error: &crate::error::Error) -> Self {
        let status = error.status_code();
        let message = match status {
            400 => "Bad Request",
            401 => "Unauthorized",
            403 => "Forbidden",
            404 => "Not Found",
            405 => "Method Not Allowed",
            413 => "Payload Too Large",
            500 | 502 => "Internal Server Error",
            _ => "Error",
        };
        Response::new(status)
            .with_header("Content-Type", "text/plain")
            .with_body(message.as_bytes().to_vec())
    }
}

/// レスポンス構築のためのビルダー
#[derive(Debug, Clone)]
pub struct ResponseBuilder {
    status: u16,
    headers: HeaderMap,
    cookies: Vec<Cookie>,
    body: Option<Bytes>,
}

impl ResponseBuilder {
    pub fn new(status: u16) -> Self {
        let mut headers = HeaderMap::new();
        inject_default_security_headers(&mut headers);
        Self {
            status,
            headers,
            cookies: Vec::new(),
            body: None,
        }
    }

    pub fn with_status(status: StatusCode) -> Self {
        Self::new(status.as_u16())
    }

    pub fn from(response: Response) -> Self {
        Self {
            status: response.status,
            headers: response.headers,
            cookies: response.cookies,
            body: response.body,
        }
    }

    pub fn header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        let key = key.into();
        let value = value.into();
        if !is_header_value_valid(&value) {
            log::warn!(
                "ResponseBuilder::header rejected invalid value for '{}': {:?}",
                key,
                value
            );
            return self;
        }
        self.headers.insert(key, value);
        self
    }

    pub fn append_header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        let key = key.into();
        let value = value.into();
        if !is_header_value_valid(&value) {
            log::warn!(
                "ResponseBuilder::append_header rejected invalid value for '{}': {:?}",
                key,
                value
            );
            return self;
        }
        self.headers.append(key, value);
        self
    }

    pub fn headers(mut self, headers: HeaderMap) -> Self {
        for (name, value) in &headers {
            self.headers.append(name.to_string(), value.to_string());
        }
        self
    }

    pub fn cookie(mut self, cookie: Cookie) -> Self {
        self.cookies.push(cookie);
        self
    }

    pub fn security_headers(mut self) -> Self {
        inject_default_security_headers(&mut self.headers);
        self
    }

    pub fn json<T: Serialize>(mut self, data: &T) -> Result<Self, Error> {
        let json = serde_json::to_vec(data)
            .map_err(|e| Error::ResponseSerializationError(e.to_string()))?;
        self.headers.insert("Content-Type", "application/json");
        self.body = Some(Bytes::from(json));
        Ok(self)
    }

    pub fn body(mut self, body: impl Into<Bytes>) -> Self {
        self.body = Some(body.into());
        self
    }

    pub fn text(mut self, text: impl Into<String>) -> Self {
        self.headers
            .insert("Content-Type", "text/plain; charset=utf-8");
        self.body = Some(Bytes::from(text.into()));
        self
    }

    pub fn html(mut self, html: impl Into<String>) -> Self {
        self.headers
            .insert("Content-Type", "text/html; charset=utf-8");
        self.body = Some(Bytes::from(html.into()));
        self
    }

    pub fn build(mut self) -> Response {
        inject_default_security_headers(&mut self.headers);
        Response {
            status: self.status,
            headers: self.headers,
            cookies: self.cookies,
            body: self.body,
        }
    }
}

fn inject_default_security_headers(map: &mut HeaderMap) {
    if !map.contains_key("X-Content-Type-Options") {
        map.append("X-Content-Type-Options", "nosniff");
    }
    if !map.contains_key("X-Frame-Options") {
        map.append("X-Frame-Options", "DENY");
    }
    if !map.contains_key("X-XSS-Protection") {
        map.append("X-XSS-Protection", "1; mode=block");
    }
    if !map.contains_key("Referrer-Policy") {
        map.append("Referrer-Policy", "strict-origin-when-cross-origin");
    }
    if !map.contains_key("Content-Security-Policy") {
        map.append("Content-Security-Policy", "default-src 'self'");
    }
}
