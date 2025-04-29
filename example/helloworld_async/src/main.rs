use runbridge::{RunBridge, common::Request, handler, error::Error};
use serde::{Serialize, Deserialize};
use std::time::Duration;

// レスポンス用の型定義
#[derive(Serialize, Deserialize)]
struct GreetingResponse {
    message: String,
    timestamp: u64,
    elapsed_ms: u64,
}

// アイテム用の型定義
#[derive(Serialize, Deserialize)]
struct Item {
    id: u32,
    name: String,
    price: f64,
}

// ベクター用のレスポンス型定義
#[derive(Serialize, Deserialize)]
struct ItemsResponse {
    items: Vec<Item>,
    count: usize,
    timestamp: u64,
}

// 文字列ベクター用のレスポンス型定義
#[derive(Serialize, Deserialize)]
struct StringsResponse {
    strings: Vec<String>,
    count: usize,
    timestamp: u64,
}

// 非同期GETリクエスト用ハンドラー関数
async fn hello_async_handler(req: Request) -> Result<GreetingResponse, Error> {
    // 開始時間を記録
    let start = std::time::Instant::now();
    
    // クエリパラメータからnameを取得
    let default_name = "World".to_string();
    let name = req.query_params.get("name").unwrap_or(&default_name);
    
    let default_lang = "en".to_string();
    let language = req.query_params.get("lang").unwrap_or(&default_lang);

    // クエリパラメータからdelayを取得（ミリ秒）
    let delay_ms = req.query_params.get("delay")
        .and_then(|d| d.parse::<u64>().ok())
        .unwrap_or(0);
    
    // 指定された時間だけ処理を遅延させる（非同期処理のシミュレーション）
    if delay_ms > 0 {
        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
    }
    
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
    
    // 処理時間を計算（ミリ秒）
    let elapsed = start.elapsed().as_millis() as u64;
    
    Ok(GreetingResponse {
        message: greeting,
        timestamp: now,
        elapsed_ms: elapsed,
    })
}

// アイテムのベクターを返す非同期ハンドラー
async fn items_async_handler(req: Request) -> Result<ItemsResponse, Error> {
    // クエリパラメータからcountを取得
    let count = req.query_params.get("count")
        .and_then(|c| c.parse::<usize>().ok())
        .unwrap_or(5);
    
    // 指定された数のアイテムを生成
    let mut items = Vec::with_capacity(count);
    for i in 1..=count {
        items.push(Item {
            id: i as u32,
            name: format!("商品 {}", i),
            price: (i as f64) * 100.0,
        });
    }
    
    // Unix timestampを取得
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let items_length = items.len();
    
    Ok(ItemsResponse {
        items,
        count: items_length,
        timestamp: now,
    })
}

// 文字列のベクターを返す非同期ハンドラー
async fn strings_async_handler(req: Request) -> Result<StringsResponse, Error> {
    // クエリパラメータからcountを取得
    let count = req.query_params.get("count")
        .and_then(|c| c.parse::<usize>().ok())
        .unwrap_or(5);
    
    // 文字列のベクターを生成
    let mut strings = Vec::with_capacity(count);
    for i in 1..=count {
        strings.push(format!("文字列 {}", i));
    }
    
    // Unix timestampを取得
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let strings_length = strings.len();
    
    Ok(StringsResponse {
        strings,
        count: strings_length,
        timestamp: now,
    })
}

// 直接文字列ベクターを返す非同期ハンドラー
async fn direct_strings_handler(req: Request) -> Result<Vec<String>, Error> {
    // クエリパラメータからcountを取得
    let count = req.query_params.get("count")
        .and_then(|c| c.parse::<usize>().ok())
        .unwrap_or(5);
    
    // 文字列のベクターを生成
    let mut strings = Vec::with_capacity(count);
    for i in 1..=count {
        strings.push(format!("直接ベクター文字列 {}", i));
    }
    
    Ok(strings)
}

// アプリケーションを構築する関数
fn create_app() -> RunBridge {
    RunBridge::builder()
        .handler(handler::async_get("/hello", hello_async_handler))
        .handler(handler::async_get("/items", items_async_handler))
        .handler(handler::async_get("/strings", strings_async_handler))
        .handler(handler::async_get("/direct-strings", direct_strings_handler))
        .build()
}

#[tokio::main]
async fn main() {
    // ロガーの初期化
    env_logger::init();
    
    // 環境に応じて実行方法を切り替え
    #[cfg(feature = "lambda")]
    {
        let app = create_app();
        if let Err(e) = runbridge::lambda::run_lambda(app).await {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }
    
    #[cfg(feature = "cloud_run")]
    {
        let port = 8080;
        let host = "0.0.0.0";
        let app = create_app();
        if let Err(e) = runbridge::cloudrun::run_cloud_run(app, host, port).await {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }

    #[cfg(feature = "cgi")]
    {
        let app = create_app();
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