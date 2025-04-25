# RunBridge

AWS LambdaとGoogle Cloud RunとCGI向けの統一的なサーバーレスAPIフレームワーク。

このライブラリは、単一のコードベースでAWS LambdaとGoogle Cloud RunそしてCGI環境の全てに対応するRustアプリケーションを開発するためのフレームワークです。actix-webに似た操作感を提供しながら、プラットフォーム固有の違いを内部で吸収します。

## 特徴

- 単一のコードベースでAWS Lambda、Google Cloud Run、そしてCGI環境の全てに対応
- actix-webに似た直感的なAPI設計
- 同期および非同期ハンドラーのサポート
- ミドルウェアによる拡張性
- 統一的なエラーハンドリング
- Cargoのfeatureによる簡単な切り替え

## インストール

Cargo.tomlに以下を追加してください：

```toml
[dependencies]
runbridge = { version = "0.1.0", features = ["cloud_run"] }  # Cloud Run向け
# または
runbridge = { version = "0.1.0", features = ["lambda"] }     # Lambda向け
# または
runbridge = { version = "0.1.0", features = ["cgi"] }        # CGI環境向け
```

## 使用例

### 基本的なハンドラー

```rust
use runbridge::{RunBridge, common::{Request, Response}, handler, error::Error};
use serde::{Serialize, Deserialize};

// レスポンス用の型定義
#[derive(Serialize, Deserialize)]
struct GreetingResponse {
    message: String,
}

// ハンドラー関数
fn hello_handler(req: Request) -> Result<GreetingResponse, Error> {
    // クエリパラメータからnameを取得
    let name = req.query_params.get("name").unwrap_or(&"World".to_string());
    
    Ok(GreetingResponse {
        message: format!("Hello, {}!", name),
    })
}

// POSTハンドラー用の入力型
#[derive(Serialize, Deserialize)]
struct CreateUserRequest {
    name: String,
    email: String,
}

// POSTハンドラー用の出力型
#[derive(Serialize, Deserialize)]
struct UserResponse {
    id: String,
    name: String,
    email: String,
}

// ユーザー作成ハンドラー
fn create_user(_req: Request, user: CreateUserRequest) -> Result<UserResponse, Error> {
    // 実際のアプリケーションではデータベースに保存する処理が入る
    Ok(UserResponse {
        id: "user_123".to_string(),
        name: user.name,
        email: user.email,
    })
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // ロガーの初期化
    env_logger::init();
    
    // アプリケーションの構築
    let app = RunBridge::builder()
        .handler(handler::get("/hello", hello_handler))
        .handler(handler::post("/users", create_user))
        .build();
    
    // 環境に応じて実行方法を切り替え
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
    
    Ok(())
}
```

### 非同期ハンドラー

非同期処理が必要な場合は、`async`ハンドラーを使用できます：

```rust
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

// アプリケーションを構築する関数
fn create_app() -> RunBridge {
    RunBridge::builder()
        .handler(handler::async_get("/hello", hello_async_handler))
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
}

## デプロイ

### AWS Lambda向け

```bash
cargo build --release --features lambda
# ビルドしたバイナリをLambda関数としてデプロイ
```

### Google Cloud Run向け

```bash
cargo build --release --features cloud_run
# ビルドしたバイナリをDockerコンテナにパッケージングしてCloud Runにデプロイ
```

### CGI環境向け

```bash
cargo build --release --features cgi --bin runbridge-cgi
# ビルドしたバイナリをCGI対応のWebサーバー（Apache, nginx+fcgi等）に配置
```

## CGI環境での実行

CGI環境でRunBridgeを利用するには、環境変数が正しく設定されていることを確認してください：

1. **必要な環境変数:**
   - `REQUEST_METHOD`: HTTPメソッド（GET, POST等）
   - `PATH_INFO`: リクエストパス
   - `QUERY_STRING`: クエリパラメータ
   - `CONTENT_TYPE`: リクエストのContent-Type（POSTリクエスト時）
   - `CONTENT_LENGTH`: リクエストボディの長さ（POSTリクエスト時）
   - `HTTP_*`: その他のHTTPヘッダー

2. **Apache設定例 (.htaccess):**
```
Options +ExecCGI
AddHandler cgi-script .cgi
DirectoryIndex index.cgi

# 全てのリクエストをindex.cgiにリダイレクト
RewriteEngine On
RewriteCond %{REQUEST_FILENAME} !-f
RewriteRule ^(.*)$ index.cgi/$1 [QSA,L]
```

3. **CGIスクリプト例 (index.cgi):**
```bash
#!/bin/bash
export PATH_INFO="${PATH_INFO:-$SCRIPT_NAME}"
./runbridge-cgi
```

## ミドルウェアの作成

カスタムミドルウェアを作成することで、認証、ロギング、リクエスト/レスポンスの変換などの機能を追加できます。

```rust
use async_trait::async_trait;
use runbridge::common::{Middleware, Request, Response};
use runbridge::error::Error;

struct LoggingMiddleware;

#[async_trait]
impl Middleware for LoggingMiddleware {
    async fn pre_process(&self, req: Request) -> Result<Request, Error> {
        println!("Received request: {} {}", req.method, req.path);
        Ok(req)
    }
    
    async fn post_process(&self, res: Response) -> Result<Response, Error> {
        println!("Sending response with status: {}", res.status);
        Ok(res)
    }
}

// ミドルウェアの使用
let app = RunBridge::builder()
    .middleware(LoggingMiddleware)
    .handler(handler::get("/hello", hello_handler))
    .build();
```

## ライセンス

MIT または Apache-2.0 