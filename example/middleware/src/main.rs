use async_trait::async_trait;
use runbridge::{RunBridge, common::{Request, Response, Middleware}, handler, error::Error};
use serde::{Serialize, Deserialize};

// 認証ミドルウェア
struct AuthMiddleware;

#[async_trait]
impl Middleware for AuthMiddleware {
    async fn pre_process(&self, req: Request) -> Result<Request, Error> {
        eprintln!("AuthMiddleware pre_process called");
        // ヘッダーから認証トークンを取得
        let token = req.headers.get("X-Auth-Token");
        match token {
            Some(t) if t == "secret-token" => Ok(req),
            _ => Err(Error::AuthorizationError("認証トークンが不正です".to_string())),
        }
    }

    async fn post_process(&self, res: Response) -> Result<Response, Error> {
        eprintln!("AuthMiddleware post_process called");
        // レスポンスをそのまま返す
        Ok(res)
    }
}

#[derive(Serialize, Deserialize)]
struct HelloResponse {
    message: String,
}

// 認証が必要なハンドラー
fn hello_handler(_req: Request) -> Result<HelloResponse, Error> {
    eprintln!("hello_handler called");
    Ok(HelloResponse {
        message: "認証成功！こんにちは！".to_string(),
    })
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    env_logger::init();
    let app = RunBridge::builder()
        .middleware(AuthMiddleware)
        .handler(handler::get("/hello", hello_handler))
        .build();

    #[cfg(feature = "lambda")]
    {
        runbridge::lambda::run_lambda(app).await?;
    }
    #[cfg(feature = "cloud_run")]
    {
        let port = 8080;
        let host = "0.0.0.0";
        runbridge::cloudrun::run_cloud_run(app, host, port).await?;
    }
    #[cfg(feature = "cgi")]
    {
        runbridge::cgi::run_cgi(app).await?;
    }

    // 環境変数がない場合、テストケースを実行
    #[cfg(not(any(feature = "lambda", feature = "cloud_run", feature = "cgi")))]
    {
        println!("テストケースを実行します");
        test_middleware().await?;
    }
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_auth_middleware() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // 認証ミドルウェアのテスト
        let middleware = AuthMiddleware;
        
        // 有効なトークンでリクエスト
        let valid_req = Request::new(runbridge::common::Method::GET, "/hello".to_string())
            .with_header("X-Auth-Token", "secret-token");
        let processed_req = middleware.pre_process(valid_req).await?;
        assert!(processed_req.headers.get("X-Auth-Token").is_some());
        
        // 無効なトークンでリクエスト
        let invalid_req = Request::new(runbridge::common::Method::GET, "/hello".to_string())
            .with_header("X-Auth-Token", "wrong-token");
        let result = middleware.pre_process(invalid_req).await;
        assert!(result.is_err());
        
        Ok(())
    }
}

// メインコードからテストするための関数
async fn test_middleware() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let middleware = AuthMiddleware;
    
    println!("有効なトークンでテスト");
    let valid_req = Request::new(runbridge::common::Method::GET, "/hello".to_string())
        .with_header("X-Auth-Token", "secret-token");
    let processed_req = middleware.pre_process(valid_req).await?;
    println!("認証成功: {:?}", processed_req.headers.get("X-Auth-Token"));
    
    println!("無効なトークンでテスト");
    let invalid_req = Request::new(runbridge::common::Method::GET, "/hello".to_string())
        .with_header("X-Auth-Token", "wrong-token");
    match middleware.pre_process(invalid_req).await {
        Ok(_) => println!("予期せぬ認証成功"),
        Err(e) => println!("認証エラー（期待通り）: {}", e),
    }
    
    // エンドツーエンドのテスト
    println!("\n統合テスト:");
    println!("RunBridgeを使ったエンドツーエンドのテスト");
    
    // 有効なトークンでのリクエストをテスト
    println!("1. 有効なトークンでのリクエスト");
    let valid_request = Request::new(runbridge::common::Method::GET, "/hello".to_string())
        .with_header("X-Auth-Token", "secret-token");
    
    // アプリケーションを構築
    let app = RunBridge::builder()
        .middleware(AuthMiddleware)
        .handler(handler::get("/hello", hello_handler))
        .build();
    
    process_test_request(&app, valid_request).await?;
    
    // 無効なトークンでのリクエストをテスト
    println!("\n2. 無効なトークンでのリクエスト");
    let invalid_request = Request::new(runbridge::common::Method::GET, "/hello".to_string())
        .with_header("X-Auth-Token", "wrong-token");
    
    process_test_request(&app, invalid_request).await?;
    
    Ok(())
}

// テストリクエストを処理するヘルパー関数
async fn process_test_request(app: &RunBridge, request: Request) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let handler = match app.find_handler(&request.path, &request.method) {
        Some(h) => h,
        None => {
            println!("  ハンドラが見つかりません");
            return Ok(());
        }
    };
    
    // ミドルウェアの前処理
    let mut req = request;
    for middleware in app.middlewares() {
        match middleware.pre_process(req).await {
            Ok(processed) => {
                req = processed;
                println!("  ミドルウェア前処理: 成功");
            },
            Err(e) => {
                println!("  ミドルウェア前処理: エラー: {}", e);
                return Ok(());
            }
        }
    }
    
    // ハンドラの実行
    match handler.handle(req).await {
        Ok(response) => {
            println!("  ハンドラ実行: 成功");
            
            // ミドルウェアの後処理
            let mut res = response;
            for middleware in app.middlewares() {
                match middleware.post_process(res).await {
                    Ok(processed) => {
                        res = processed;
                        println!("  ミドルウェア後処理: 成功");
                    },
                    Err(e) => {
                        println!("  ミドルウェア後処理: エラー: {}", e);
                        return Ok(());
                    }
                }
            }
            
            // レスポンスの確認
            if let Some(body) = res.body {
                match String::from_utf8(body) {
                    Ok(body_str) => println!("  レスポンスボディ: {}", body_str),
                    Err(_) => println!("  レスポンスボディ: (バイナリデータ)"),
                }
            } else {
                println!("  レスポンスボディ: なし");
            }
        },
        Err(e) => {
            println!("  ハンドラ実行: エラー: {}", e);
        }
    }
    
    Ok(())
}
