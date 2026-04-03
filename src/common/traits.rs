//! コアトレイト定義（Handler、Middleware）

use super::http::{Request, Response};
use crate::error::Error;
use crate::error_handler::ErrorHandler;
use async_trait::async_trait;
use std::sync::Arc;

/// ハンドラーの特性
#[async_trait]
pub trait Handler: Send + Sync {
    /// リクエストを処理
    async fn handle(&self, req: Request) -> Result<Response, Error>;
}

/// ミドルウェアの特性
#[async_trait]
pub trait Middleware: Send + Sync {
    /// リクエストを包み込み、必要なら `next` を呼ぶ
    async fn handle(&self, req: Request, next: Next<'_>) -> Result<Response, Error>;
}

/// 次のミドルウェアまたは最終ハンドラー
pub struct Next<'a> {
    pub(crate) middlewares: &'a [Arc<dyn Middleware>],
    pub(crate) endpoint: &'a dyn Handler,
    pub(crate) error_handler: &'a dyn ErrorHandler,
    pub(crate) error_request: &'a Request,
}

impl<'a> Next<'a> {
    /// 次のミドルウェアまたは最終ハンドラーを実行
    pub async fn run(self, req: Request) -> Result<Response, Error> {
        let result = if let Some((current, rest)) = self.middlewares.split_first() {
            current
                .handle(
                    req,
                    Next {
                        middlewares: rest,
                        endpoint: self.endpoint,
                        error_handler: self.error_handler,
                        error_request: self.error_request,
                    },
                )
                .await
        } else {
            self.endpoint.handle(req).await
        };

        match result {
            Ok(response) => Ok(response),
            Err(error) => Ok(self.error_handler.handle(self.error_request, &error).await),
        }
    }
}
