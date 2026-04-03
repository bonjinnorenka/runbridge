# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0] - 2026-04-03

### Added

- Path template routing (e.g. `/users/{id}`) with segment scoring and duplicate-route detection on flatten.
- `Router`, `RunBridgeBuilder::router`, and `nest` for composing routes under a prefix.
- `RunBridgeBuilder::state` and `State<T>` extractor for shared application state.
- `RunBridgeBuilder::fallback` for global 404 handling when no route matches.
- `RunBridgeBuilder::error_handler` and `error_handler::ErrorHandler` trait for customizing responses from `Err(Error)` in handlers and middleware (not applied to static 404/405 responses).
- `Cors` middleware with preflight short-circuit and `try_validate()` for unsafe combinations.
- `fixed_file`, `fixed_file_with`, and `FixedFileOptions` for serving a small set of static bytes at fixed paths (loaded when the app is built, not per request).
- Extractors: `Path<T>`, `Query<T>`, `Json<T>`, `Form<T>`, `TextBody`, `BytesBody` (see `handler::extractors`).
- `handler::patch`, `handler::head`, and async/`try_*` variants where applicable.
- `RunBridge::resolve` returning `RouteMatch` (`Matched`, `MethodNotAllowed` with `allow` methods, `NotFound`).
- Automatic `Allow` header on HTTP 405 Method Not Allowed.
- `Method::ALL`, `Hash`, and `FromStr` for `Method`.
- `bytes::Bytes` for request/response bodies.
- Dependency on `serde_html_form` for form URL-encoded bodies.

### Changed

- **Breaking:** `Request::query_params` replaced by `Request::query` (`QueryMap`, multi-value).
- **Breaking:** `Request::headers` is now `HeaderMap` (case-insensitive, multi-value) instead of `HashMap<String, String>`.
- **Breaking:** `Request` includes `path_params`, `cookies`, and `body: Option<Bytes>`.
- **Breaking:** `Response::headers` is `HeaderMap`; `Set-Cookie` is modeled via `Response::cookies: Vec<Cookie>` instead of raw headers alone.
- **Breaking:** `Handler` no longer has `matches` or `path_pattern`; routing is owned by `Route` and the builder. Handlers implement only `async fn handle(&self, req: Request) -> Result<Response, Error>`.
- **Breaking:** `Middleware` uses `async fn handle(&self, req: Request, next: Next<'_>) -> Result<Response, Error>` instead of `pre_process` / `post_process`.
- **Breaking:** `RunBridgeBuilder::handler` accepts `handler::Route` from helpers like `handler::get(...)` rather than a boxed custom `Handler` with embedded regex path.
- Route registration uses path templates; regex-based path patterns are removed.
- JSON `Content-Type` detection and header key normalization behavior updated (see tests and `common::http`).

### Removed

- **Breaking:** `RunBridge::find_handler` — use `RunBridge::resolve` and `handle_request`, or inspect `RouteMatch` as needed.

### Fixed

- Gzip-compressed request body decompression when `Content-Encoding: gzip` is set.
- CGI: multiple `Set-Cookie` lines and output buffering issues.
- Various test and integration fixes across Lambda, Cloud Run, and CGI adapters.

### Security

- Central maximum body size and safer cloning of `RequestContext`.
- Header and cookie value validation (reject control characters and CRLF injection).
- ReDoS-hardened path/template handling and safer regex usage where applicable.
- Default security headers injected on new responses where not already present.
