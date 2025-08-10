use std::future::Future;
use std::marker::PhantomData;
use std::sync::OnceLock;

use async_trait::async_trait;
use log::{debug, error, info, warn};
use regex::Regex;
use serde::de::DeserializeOwned;

#[cfg(debug_assertions)]
use std::time::{Duration, Instant};

use crate::common::{Handler, Method, Request, Response};
use crate::error::Error;

use super::body::is_json_like_content_type;
use super::pattern::ensure_safe_pattern;
use super::response::ResponseWrapper;

/// ルートハンドラー
pub struct RouteHandler<F, T, R>
where
    F: Fn(Request, Option<T>) -> Result<R, Error> + Send + Sync + 'static,
    T: DeserializeOwned + Send + Sync + 'static,
    R: ResponseWrapper + Send + Sync + 'static,
{
    /// ルートパス（正規表現パターン）
    pub path_pattern: String,
    /// コンパイル済み正規表現（キャッシュ）
    pub compiled_regex: OnceLock<Result<Regex, regex::Error>>,
    /// HTTPメソッド
    pub method: Method,
    /// ハンドラー関数
    pub handler_fn: F,
    /// リクエストボディの型
    pub _request_type: PhantomData<T>,
    /// レスポンスボディの型
    pub _response_type: PhantomData<R>,
}

impl<F, T, R> RouteHandler<F, T, R>
where
    F: Fn(Request, Option<T>) -> Result<R, Error> + Send + Sync + 'static,
    T: DeserializeOwned + Send + Sync + 'static,
    R: ResponseWrapper + Send + Sync + 'static,
{
    /// 新しいRouteHandlerを作成
    pub fn try_new(
        method: Method,
        path_pattern: impl Into<String>,
        handler_fn: F,
    ) -> Result<Self, Error> {
        let pattern = path_pattern.into();

        // パターンの安全性チェック
        let safe_pattern = ensure_safe_pattern(&pattern)?;

        // 開発時はinfo、本番相当ではdebugに落とす
        #[cfg(debug_assertions)]
        info!(
            "Registering handler for {} with pattern: {}",
            method, safe_pattern
        );
        #[cfg(not(debug_assertions))]
        debug!(
            "Registering handler for {} with pattern: {}",
            method, safe_pattern
        );
        Ok(Self {
            method,
            path_pattern: safe_pattern,
            compiled_regex: OnceLock::new(),
            handler_fn,
            _request_type: PhantomData,
            _response_type: PhantomData,
        })
    }

    /// 新しいRouteHandlerを作成（従来のAPI、非推奨）
    #[deprecated(note = "Use try_new instead for better error handling")]
    pub fn new(method: Method, path_pattern: impl Into<String>, handler_fn: F) -> Self {
        Self::try_new(method, path_pattern, handler_fn).unwrap_or_else(|e| {
            panic!("Failed to create RouteHandler: {}", e);
        })
    }
}

/// 非同期ルートハンドラー
pub struct AsyncRouteHandler<F, T, R, Fut>
where
    F: Fn(Request, Option<T>) -> Fut + Send + Sync + 'static,
    T: DeserializeOwned + Send + Sync + 'static,
    R: ResponseWrapper + Send + Sync + 'static,
    Fut: Future<Output = Result<R, Error>> + Send + Sync + 'static,
{
    /// ルートパス（正規表現パターン）
    pub path_pattern: String,
    /// コンパイル済み正規表現（キャッシュ）
    pub compiled_regex: OnceLock<Result<Regex, regex::Error>>,
    /// HTTPメソッド
    pub method: Method,
    /// 非同期ハンドラー関数
    pub handler_fn: F,
    /// リクエストボディの型
    pub _request_type: PhantomData<T>,
    /// レスポンスボディの型
    pub _response_type: PhantomData<R>,
    /// Future型
    pub _future_type: PhantomData<Fut>,
}

impl<F, T, R, Fut> AsyncRouteHandler<F, T, R, Fut>
where
    F: Fn(Request, Option<T>) -> Fut + Send + Sync + 'static,
    T: DeserializeOwned + Send + Sync + 'static,
    R: ResponseWrapper + Send + Sync + 'static,
    Fut: Future<Output = Result<R, Error>> + Send + Sync + 'static,
{
    /// 新しいAsyncRouteHandlerを作成
    pub fn try_new(
        method: Method,
        path_pattern: impl Into<String>,
        handler_fn: F,
    ) -> Result<Self, Error> {
        let pattern = path_pattern.into();

        // パターンの安全性チェック
        let safe_pattern = ensure_safe_pattern(&pattern)?;

        // 開発時はinfo、本番相当ではdebugに落とす
        #[cfg(debug_assertions)]
        info!(
            "Registering async handler for {} with pattern: {}",
            method, safe_pattern
        );
        #[cfg(not(debug_assertions))]
        debug!(
            "Registering async handler for {} with pattern: {}",
            method, safe_pattern
        );
        Ok(Self {
            method,
            path_pattern: safe_pattern,
            compiled_regex: OnceLock::new(),
            handler_fn,
            _request_type: PhantomData,
            _response_type: PhantomData,
            _future_type: PhantomData,
        })
    }

    /// 新しいAsyncRouteHandlerを作成（従来のAPI、非推奨）
    #[deprecated(note = "Use try_new instead for better error handling")]
    pub fn new(method: Method, path_pattern: impl Into<String>, handler_fn: F) -> Self {
        Self::try_new(method, path_pattern, handler_fn).unwrap_or_else(|e| {
            panic!("Failed to create AsyncRouteHandler: {}", e);
        })
    }
}

#[async_trait]
impl<F, T, R> Handler for RouteHandler<F, T, R>
where
    F: Fn(Request, Option<T>) -> Result<R, Error> + Send + Sync + 'static,
    T: DeserializeOwned + Send + Sync + 'static,
    R: ResponseWrapper + Send + Sync + 'static,
{
    fn matches(&self, path: &str, method: &Method) -> bool {
        if method != &self.method {
            return false;
        }

        // コンパイル済み正規表現を取得またはコンパイル
        let compiled_result = self.compiled_regex.get_or_init(|| Regex::new(&self.path_pattern));

        match compiled_result {
            Ok(regex) => {
                // デバッグビルド時のみタイムアウト監視
                #[cfg(debug_assertions)]
                {
                    let start_time = Instant::now();
                    let is_match = regex.is_match(path);
                    let elapsed = start_time.elapsed();

                    if elapsed > Duration::from_millis(100) {
                        warn!(
                            "Slow regex matching detected: pattern '{}' took {:?} for path '{}'",
                            self.path_pattern, elapsed, path
                        );
                    }

                    debug!(
                        "Path matching: {} against pattern {}: {} (took {:?})",
                        path, self.path_pattern, is_match, elapsed
                    );
                    is_match
                }
                #[cfg(not(debug_assertions))]
                {
                    let is_match = regex.is_match(path);
                    debug!(
                        "Path matching: {} against pattern {}: {}",
                        path, self.path_pattern, is_match
                    );
                    is_match
                }
            }
            Err(e) => {
                error!(
                    "Invalid regex pattern: {} - {}. Pattern will be rejected for security.",
                    self.path_pattern, e
                );
                false
            }
        }
    }

    fn path_pattern(&self) -> &str {
        &self.path_pattern
    }

    async fn handle(&self, req: Request) -> Result<Response, Error> {
        // リクエストボディが長さ>0のときのみContent-Type検証とJSONパースを行う
        let has_non_empty_body = req.body.as_ref().map(|b| !b.is_empty()).unwrap_or(false);
        let body_data = if has_non_empty_body {
            // 取込み時にヘッダーは小文字化されている前提
            let content_type = req.headers.get("content-type").cloned();

            let ct = content_type.ok_or_else(|| {
                warn!("Request with body missing Content-Type header");
                Error::InvalidRequestBody("Missing Content-Type header".to_string())
            })?;

            if !is_json_like_content_type(&ct) {
                warn!("Unsupported Content-Type for JSON parsing: {}", ct);
                return Err(Error::InvalidRequestBody(format!(
                    "Unsupported Content-Type: {} (expected application/json or *+json)",
                    ct
                )));
            }

            Some(req.json::<T>()?)
        } else {
            None
        };

        let result = (self.handler_fn)(req, body_data)?;
        result.into_response()
    }
}

#[async_trait]
impl<F, T, R, Fut> Handler for AsyncRouteHandler<F, T, R, Fut>
where
    F: Fn(Request, Option<T>) -> Fut + Send + Sync + 'static,
    T: DeserializeOwned + Send + Sync + 'static,
    R: ResponseWrapper + Send + Sync + 'static,
    Fut: Future<Output = Result<R, Error>> + Send + Sync + 'static,
{
    fn matches(&self, path: &str, method: &Method) -> bool {
        if method != &self.method {
            return false;
        }

        // コンパイル済み正規表現を取得またはコンパイル
        let compiled_result = self.compiled_regex.get_or_init(|| Regex::new(&self.path_pattern));

        match compiled_result {
            Ok(regex) => {
                // デバッグビルド時のみタイムアウト監視
                #[cfg(debug_assertions)]
                {
                    let start_time = Instant::now();
                    let is_match = regex.is_match(path);
                    let elapsed = start_time.elapsed();

                    if elapsed > Duration::from_millis(100) {
                        warn!(
                            "Slow regex matching detected: pattern '{}' took {:?} for path '{}'",
                            self.path_pattern, elapsed, path
                        );
                    }

                    debug!(
                        "Path matching: {} against pattern {}: {} (took {:?})",
                        path, self.path_pattern, is_match, elapsed
                    );
                    is_match
                }
                #[cfg(not(debug_assertions))]
                {
                    let is_match = regex.is_match(path);
                    debug!(
                        "Path matching: {} against pattern {}: {}",
                        path, self.path_pattern, is_match
                    );
                    is_match
                }
            }
            Err(e) => {
                error!(
                    "Invalid regex pattern: {} - {}. Pattern will be rejected for security.",
                    self.path_pattern, e
                );
                false
            }
        }
    }

    fn path_pattern(&self) -> &str {
        &self.path_pattern
    }

    async fn handle(&self, req: Request) -> Result<Response, Error> {
        // リクエストボディが長さ>0のときのみContent-Type検証とJSONパースを行う
        let has_non_empty_body = req.body.as_ref().map(|b| !b.is_empty()).unwrap_or(false);
        let body_data = if has_non_empty_body {
            // 取込み時にヘッダーは小文字化されている前提
            let content_type = req.headers.get("content-type").cloned();

            let ct = content_type.ok_or_else(|| {
                warn!("Request with body missing Content-Type header");
                Error::InvalidRequestBody("Missing Content-Type header".to_string())
            })?;

            if !is_json_like_content_type(&ct) {
                warn!("Unsupported Content-Type for JSON parsing: {}", ct);
                return Err(Error::InvalidRequestBody(format!(
                    "Unsupported Content-Type: {} (expected application/json or *+json)",
                    ct
                )));
            }

            Some(req.json::<T>()?)
        } else {
            None
        };

        let result = (self.handler_fn)(req, body_data).await?;
        result.into_response()
    }
}

