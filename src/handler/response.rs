use serde::Serialize;

use crate::common::Response;
use crate::error::Error;

/// レスポンス変換トレイト
pub trait ResponseWrapper {
    /// 自身をResponseに変換
    fn into_response(self) -> Result<Response, Error>;
}

/// 通常のシリアライズ可能なデータ型に対するResponseWrapper実装
impl<T: Serialize> ResponseWrapper for T {
    fn into_response(self) -> Result<Response, Error> {
        Response::ok().json(&self)
    }
}

/// Response型に対するResponseWrapper実装（恒等関数）
impl ResponseWrapper for Response {
    fn into_response(self) -> Result<Response, Error> {
        Ok(self)
    }
}

