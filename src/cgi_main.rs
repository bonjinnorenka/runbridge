//! CGI環境でのエントリポイント
//!
//! CGI環境で実行される際のメインプログラム

use env_logger::Env;
use log::{error, info};
use runbridge::{cgi, RunBridge};

// サンプルハンドラの実装
mod sample_handler;

#[tokio::main]
async fn main() {
    // ログ設定（標準エラー出力に出力）
    // CGIでは標準出力がHTTPレスポンスとなるため、ログは標準エラー出力に出力する
    env_logger::Builder::from_env(Env::default().default_filter_or("info"))
        .target(env_logger::Target::Stderr)
        .init();
    
    info!("Starting RunBridge CGI application");
    
    // アプリケーションの構築
    let app = RunBridge::builder()
        .handler(sample_handler::HelloHandler::new())
        .handler(sample_handler::EchoHandler::new())
        .handler(sample_handler::PanicHandler::new())
        .build();
    
    // CGI処理の実行
    if let Err(err) = cgi::run_cgi(app).await {
        error!("Error running CGI application: {:?}", err);
        std::process::exit(1);
    }
    
    info!("CGI request processed successfully");
} 