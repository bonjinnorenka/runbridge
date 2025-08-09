//! コアトレイト定義（Handler、Middleware）

use async_trait::async_trait;
use crate::error::Error;
use super::http::{Request, Response, Method};

/// ハンドラーの特性
#[async_trait]
pub trait Handler: Send + Sync {
    /// パスとメソッドがこのハンドラにマッチするかどうかを判定
    fn matches(&self, path: &str, method: &Method) -> bool;
    
    /// ハンドラに関連付けられたパスパターン文字列を取得
    fn path_pattern(&self) -> &str;

    /// リクエストを処理
    async fn handle(&self, req: Request) -> Result<Response, Error>;
}

/// ミドルウェアの特性
#[async_trait]
pub trait Middleware: Send + Sync {
    /// リクエスト前の処理
    async fn pre_process(&self, req: Request) -> Result<Request, Error>;
    
    /// レスポンス後の処理
    async fn post_process(&self, res: Response) -> Result<Response, Error>;
}