use runbridge::{RunBridge, common::{Request}, handler, error::Error};
use serde::{Serialize, Deserialize};

// レスポンス用の型定義
#[derive(Serialize, Deserialize)]
struct GreetingResponse {
    message: String,
    timestamp: u64,
}

// GETリクエスト用ハンドラー関数
fn hello_handler(req: Request) -> Result<GreetingResponse, Error> {
    // クエリパラメータからnameを取得（一時オブジェクト問題を回避するためにletで変数を作成）
    let default_name = "World".to_string();
    let name = req.query_params.get("name").unwrap_or(&default_name);
    
    let default_lang = "en".to_string();
    let language = req.query_params.get("lang").unwrap_or(&default_lang);
    
    // 言語に基づいて挨拶を変更
    let greeting = match language.as_str() {
        "ja" => format!("こんにちは、{}!", name),
        "fr" => format!("Bonjour, {} !", name),
        "es" => format!("¡Hola, {}!", name),
        "de" => format!("Hallo, {}!", name),
        _ => format!("Hello, {}!", name),
    };
    
    // Unix timestampを取得
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    
    Ok(GreetingResponse {
        message: greeting,
        timestamp: now,
    })
}

#[tokio::main]
async fn main() {
    // ロガーの初期化
    env_logger::init();
    
    // アプリケーションの構築
    let app = RunBridge::builder()
        .handler(handler::get("/hello", hello_handler))
        .build();
    
    // 環境に応じて実行方法を切り替え
    #[cfg(feature = "lambda")]
    {
        if let Err(e) = runbridge::lambda::run_lambda(app).await {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }
    
    #[cfg(feature = "cloud_run")]
    {
        let port = 8080;
        let host = "0.0.0.0";
        if let Err(e) = runbridge::cloudrun::run_cloud_run(app, host, port).await {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }

    #[cfg(feature = "cgi")]
    {
        if let Err(e) = runbridge::cgi::run_cgi(app).await {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }
    
    // どちらのfeatureも有効でない場合はエラーメッセージを表示
    #[cfg(not(any(feature = "lambda", feature = "cloud_run", feature = "cgi")))]
    {
        println!("Error: Neither 'lambda' nor 'cloud_run' feature is enabled.");
        println!("Please build with either:");
        println!("  cargo build --features lambda");
        println!("  or");
        println!("  cargo build --features cloud_run");
        println!("  or");
        println!("  cargo build --features cgi");
        std::process::exit(1);
    }
} 
