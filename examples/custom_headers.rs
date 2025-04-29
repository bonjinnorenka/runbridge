use runbridge::{
    common::{Request, Response},
    error::Error,
    handler::get,
    RunBridge,
};
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize)]
struct ApiResponse {
    message: String,
    status: String,
}

// 通常のJSONレスポンスを返すハンドラー
fn normal_handler(_req: Request) -> Result<ApiResponse, Error> {
    Ok(ApiResponse {
        message: "This is a normal JSON response".to_string(),
        status: "success".to_string(),
    })
}

// カスタムヘッダーを設定したレスポンスを返すハンドラー
fn custom_header_handler(_req: Request) -> Result<Response, Error> {
    let data = ApiResponse {
        message: "This response includes custom headers".to_string(),
        status: "success".to_string(),
    };
    
    // まずResponseを作成し、ヘッダーを追加してからJSONボディを設定
    Ok(Response::ok()
        .with_header("X-API-Version", "1.0")
        .with_header("X-Rate-Limit", "100")
        .with_header("X-Rate-Limit-Reset", "3600")
        .json(&data)?)
}

// CORSヘッダーを追加する例
fn cors_handler(_req: Request) -> Result<Response, Error> {
    let data = ApiResponse {
        message: "This response includes CORS headers".to_string(),
        status: "success".to_string(),
    };
    
    Ok(Response::ok()
        .with_header("Access-Control-Allow-Origin", "*")
        .with_header("Access-Control-Allow-Methods", "GET, POST, OPTIONS")
        .with_header("Access-Control-Allow-Headers", "Content-Type, Authorization")
        .json(&data)?)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // ロガーを初期化
    env_logger::init();

    // アプリケーションを構築
    let app = RunBridge::builder()
        .handler(get("/api/normal", normal_handler))
        .handler(get("/api/custom-headers", custom_header_handler))
        .handler(get("/api/cors", cors_handler))
        .build();
    
    println!("サーバーを起動中...");
    println!("次のエンドポイントにアクセスしてみてください:");
    println!("- http://localhost:8080/api/normal");
    println!("- http://localhost:8080/api/custom-headers");
    println!("- http://localhost:8080/api/cors");
    
    // 実行環境に応じて処理を分岐
    #[cfg(feature = "cloud_run")]
    {
        // Cloud Run環境での実行
        runbridge::cloudrun::run_cloud_run(app, "127.0.0.1", 8080).await?;
    }
    
    #[cfg(feature = "lambda")]
    {
        // Lambda環境での実行
        println!("Lambda環境では、このサンプルを直接実行することはできません。");
        println!("AWS Lambdaにデプロイして実行してください。");
    }
    
    #[cfg(feature = "cgi")]
    {
        // CGI環境での実行
        println!("CGI環境では、このサンプルを直接実行することはできません。");
        println!("Webサーバー経由でCGIとして実行してください。");
    }
    
    #[cfg(not(any(feature = "cloud_run", feature = "lambda", feature = "cgi")))]
    {
        println!("実行するには、次のいずれかの機能を有効にしてビルドしてください:");
        println!("  cargo run --example custom_headers --features cloud_run");
        println!("  cargo run --example custom_headers --features lambda");
        println!("  cargo run --example custom_headers --features cgi");
    }
    
    Ok(())
} 