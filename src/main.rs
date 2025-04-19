use env_logger;
use log::info;
use serde::{Serialize, Deserialize};
use std::env;

use runbridge::{RunBridge, common::Request, handler, error::Error};

#[derive(Serialize, Deserialize)]
struct Item {
    id: String,
    name: String,
    description: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct ItemList {
    items: Vec<Item>,
}

// サンプルのGETルートハンドラー
fn health_handler(_req: Request) -> Result<serde_json::Value, Error> {
    Ok(serde_json::json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

// サンプルのGETアイテム一覧ハンドラー
fn get_items(_req: Request) -> Result<ItemList, Error> {
    // 仮のアイテムリストを返却
    let items = vec![
        Item {
            id: "1".to_string(),
            name: "Item 1".to_string(),
            description: Some("Description for item 1".to_string()),
        },
        Item {
            id: "2".to_string(),
            name: "Item 2".to_string(),
            description: None,
        },
    ];

    Ok(ItemList { items })
}

// 新しいアイテムを作成するハンドラー
fn create_item(_req: Request, item: Item) -> Result<Item, Error> {
    // 実際のアプリケーションではデータベースに保存する処理が入る
    info!("Creating new item: {}", item.name);
    
    Ok(item)
}

#[tokio::main]
async fn main() {
    // ロガーの初期化
    env_logger::init();

    // アプリケーションの構築
    let _app = RunBridge::builder()
        .handler(handler::get("^/$", health_handler))
        .handler(handler::get("^/items$", get_items))
        .handler(handler::post("^/items$", create_item))
        .build();

    info!("Starting RunBridge application");

    // 環境に応じて実行方法を切り替え
    #[cfg(feature = "lambda")]
    {
        info!("Running as AWS Lambda");
        if let Err(e) = runbridge::lambda::run_lambda(app).await {
            eprintln!("Lambda error: {}", e);
            std::process::exit(1);
        }
    }

    #[cfg(feature = "cloud_run")]
    {
        let port = match env::var("PORT").unwrap_or_else(|_| "8080".to_string()).parse::<u16>() {
            Ok(p) => p,
            Err(e) => {
                eprintln!("Error parsing port: {}", e);
                std::process::exit(1);
            }
        };
        let host = "0.0.0.0";
        info!("Running as HTTP server on port {}", port);
        if let Err(e) = runbridge::cloudrun::run_cloud_run(app, host, port).await {
            eprintln!("Cloud Run error: {}", e);
            std::process::exit(1);
        }
    }

    #[cfg(not(any(feature = "lambda", feature = "cloud_run")))]
    {
        println!("Please enable either 'lambda' or 'cloud_run' feature to run the application.");
        println!("Example: cargo run --features cloud_run");
        std::process::exit(1);
    }
}
