//! CGI環境での実行をサポートするモジュール
//!
//! 環境変数と標準入力からリクエストを構築し、
//! 標準出力にHTTPレスポンスフォーマットで出力するための機能を提供します。

pub mod validation;
pub mod error_logging;
pub mod request;
pub mod response;
pub mod core;

// 互換性維持のためのパブリックAPI再エクスポート
pub use core::run_cgi;

#[cfg(test)]
mod tests;