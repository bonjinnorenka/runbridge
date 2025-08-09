//! RunBridge: AWS LambdaとGoogle Cloud Run向けの統一サーバレスAPIフレームワーク
//!
//! 単一のコードベースで異なるサーバレス環境に対応するためのライブラリ

// --- Feature validation -----------------------------------------------------
// 競合するfeatureが同時に有効化されている場合はコンパイルエラーを出す。
// 対象: "lambda" / "cloud_run" / "cgi"

// 2つ以上のターゲット実行環境featureが同時に有効化された場合（いずれの組み合わせでも）エラー
#[cfg(all(feature = "lambda", feature = "cloud_run"))]
compile_error!(
    "Conflicting features: 'lambda' and 'cloud_run' cannot be enabled together. Choose exactly one."
);

#[cfg(all(feature = "lambda", feature = "cgi"))]
compile_error!(
    "Conflicting features: 'lambda' and 'cgi' cannot be enabled together. Choose exactly one."
);

#[cfg(all(feature = "cloud_run", feature = "cgi"))]
compile_error!(
    "Conflicting features: 'cloud_run' and 'cgi' cannot be enabled together. Choose exactly one."
);

// どれも選ばれていない場合は警告を出す（ビルドは継続）
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
    // 非推奨定数を参照して警告を発生させる（コンパイルは成功）
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

/// リクエストを処理するアプリケーションを構築するためのビルダー
pub struct RunBridgeBuilder {
    handlers: Vec<Box<dyn common::Handler>>,
    middlewares: Vec<Box<dyn common::Middleware>>,
}

impl Default for RunBridgeBuilder {
    fn default() -> Self {
        Self {
            handlers: Vec::new(),
            middlewares: Vec::new(),
        }
    }
}

impl RunBridgeBuilder {
    /// 新しいRunBridgeBuilderインスタンスを作成
    pub fn new() -> Self {
        Self::default()
    }

    /// ハンドラを追加
    pub fn handler<H>(mut self, handler: H) -> Self 
    where 
        H: common::Handler + 'static
    {
        self.handlers.push(Box::new(handler));
        // ハンドラーを追加するたびにパスの `/` の数で降順ソート
        self.handlers.sort_unstable_by(|a, b| {
            let count_a = a.path_pattern().matches('/').count();
            let count_b = b.path_pattern().matches('/').count();
            // 降順ソート (多い方が先)
            count_b.cmp(&count_a)
        });
        self
    }

    /// ミドルウェアを追加
    pub fn middleware<M>(mut self, middleware: M) -> Self
    where
        M: common::Middleware + 'static
    {
        self.middlewares.push(Box::new(middleware));
        self
    }

    /// アプリケーションをビルドして返却
    pub fn build(self) -> RunBridge {
        RunBridge {
            handlers: self.handlers,
            middlewares: self.middlewares,
        }
    }
}

/// リクエストを処理するアプリケーション
pub struct RunBridge {
    handlers: Vec<Box<dyn common::Handler>>,
    middlewares: Vec<Box<dyn common::Middleware>>,
}

impl RunBridge {
    /// 新しいRunBridgeBuilderインスタンスを作成
    pub fn builder() -> RunBridgeBuilder {
        RunBridgeBuilder::new()
    }

    /// 指定されたパスにマッチするハンドラを取得
    pub fn find_handler(&self, path: &str, method: &common::Method) -> Option<&Box<dyn common::Handler>> {
        self.handlers.iter().find(|handler| handler.matches(path, method))
    }

    /// ミドルウェアのリストを取得
    pub fn middlewares(&self) -> &[Box<dyn common::Middleware>] {
        &self.middlewares
    }
} 
