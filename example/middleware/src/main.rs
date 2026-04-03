use async_trait::async_trait;
use runbridge::{
    RunBridge,
    common::{Middleware, Next, Request, Response},
    handler,
    error::Error,
};
use serde::{Serialize, Deserialize};

// 認証ミドルウェア
struct AuthMiddleware;

#[async_trait]
impl Middleware for AuthMiddleware {
    async fn handle(&self, req: Request, next: Next<'_>) -> Result<Response, Error> {
        eprintln!("AuthMiddleware handle called");
        let token = req.headers.get("X-Auth-Token");
        match token {
            Some(t) if t == "secret-token" => {
                let res = next.run(req).await?;
                eprintln!("AuthMiddleware: レスポンスを返却");
                Ok(res)
            }
            _ => Err(Error::AuthorizationError("認証トークンが不正です".to_string())),
        }
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
        let app = RunBridge::builder()
            .middleware(AuthMiddleware)
            .handler(handler::get("/hello", hello_handler))
            .build();

        let valid_req = Request::new(runbridge::Method::GET, "/hello".to_string())
            .with_header("X-Auth-Token", "secret-token");
        let res = app.handle_request(valid_req).await;
        assert_eq!(res.status, 200);

        let invalid_req = Request::new(runbridge::Method::GET, "/hello".to_string())
            .with_header("X-Auth-Token", "wrong-token");
        let res = app.handle_request(invalid_req).await;
        assert_eq!(res.status, 403);

        Ok(())
    }
}

// ターゲット feature が無いときだけ main から呼ぶ（cgi 等が有効なビルドでは未使用）
#[cfg(not(any(feature = "lambda", feature = "cloud_run", feature = "cgi")))]
async fn test_middleware() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let app = RunBridge::builder()
        .middleware(AuthMiddleware)
        .handler(handler::get("/hello", hello_handler))
        .build();

    println!("有効なトークンでテスト");
    let valid_req = Request::new(runbridge::Method::GET, "/hello".to_string())
        .with_header("X-Auth-Token", "secret-token");
    let res = app.handle_request(valid_req).await;
    println!("  status: {}", res.status);
    if let Some(body) = res.body {
        println!("  body: {}", String::from_utf8_lossy(&body));
    }

    println!("無効なトークンでテスト");
    let invalid_req = Request::new(runbridge::Method::GET, "/hello".to_string())
        .with_header("X-Auth-Token", "wrong-token");
    let res = app.handle_request(invalid_req).await;
    println!("  status: {} (期待: 403)", res.status);

    Ok(())
}
