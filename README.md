# RunBridge

AWS Lambda / Google Cloud Run / CGI 向けの統一サーバーレス API フレームワークです。

P0 では core を再設計し、`Request` / `Response` / routing / middleware / extractor の基盤を入れ替えました。routing は path template 中心、middleware は `next` chain、request/response は multi-value header/query と cookie を first-class に扱います。

## 主な変更点

- routing は regex ではなく path template を使います
  - 例: `"/users/{id}"`, `"/posts/{post_id}/comments/{comment_id}"`
- `Request` は `query`, `headers`, `path_params`, `cookies`, `body`, `context` を持ちます
- `Response` は `headers` と `cookies` を分離して持ちます
- middleware は `pre_process/post_process` ではなく `handle(req, next)` です
- app state は `RunBridge::builder().state(Arc<T>)` で注入します
- `405 Method Not Allowed` では自動で `Allow` ヘッダーを返します
- global fallback は `RunBridge::builder().fallback(...)` で設定します

## インストール

```toml
[dependencies]
runbridge = { version = "0.1.1", features = ["cloud_run"] }
# または
runbridge = { version = "0.1.1", features = ["lambda"] }
# または
runbridge = { version = "0.1.1", features = ["cgi"] }
```

## 基本例

```rust
use runbridge::{handler, Method, Request, Response, RunBridge};
use runbridge::error::Error;
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
struct GreetingResponse {
    message: String,
}

#[derive(Deserialize)]
struct CreateUserRequest {
    name: String,
    email: String,
}

fn hello_handler(req: Request) -> Result<GreetingResponse, Error> {
    let name = req.query.get("name").unwrap_or("World");
    Ok(GreetingResponse {
        message: format!("Hello, {}!", name),
    })
}

fn create_user(_req: Request, body: CreateUserRequest) -> Result<Response, Error> {
    Ok(Response::created().json(&serde_json::json!({
        "id": "user_123",
        "name": body.name,
        "email": body.email,
    }))?)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    let app = RunBridge::builder()
        .handler(handler::get("/hello", hello_handler))
        .handler(handler::post("/users", create_user))
        .build();

    #[cfg(feature = "lambda")]
    {
        runbridge::lambda::run_lambda(app).await?;
    }

    #[cfg(feature = "cloud_run")]
    {
        runbridge::cloudrun::run_cloud_run(app, "0.0.0.0", 8080).await?;
    }

    #[cfg(feature = "cgi")]
    {
        runbridge::cgi::run_cgi(app).await?;
    }

    Ok(())
}
```

## Routing

path template を使います。

```rust
use async_trait::async_trait;
use runbridge::{handler, Handler, Method, Request, Response, RunBridge};
use runbridge::error::Error;

struct UserHandler;

#[async_trait]
impl Handler for UserHandler {
    async fn handle(&self, req: Request) -> Result<Response, Error> {
        let id = req.path_params.get("id").cloned().unwrap_or_default();
        Ok(Response::ok().with_body(format!("user: {}", id).into_bytes()))
    }
}

let app = RunBridge::builder()
    .handler(handler::route(Method::GET, "/users/{id}", UserHandler))
    .build();
```

## Request / Response

`Request`:

- `query.get("x")` で最初の値を取得
- `query.get_all("x")` で複数値を取得
- `headers.get("content-type")` は大小文字を無視
- `path_params` には router が抽出した値が入る
- `cookies` は request cookie の統一表現

`Response`:

- `with_header` は単一値ヘッダー
- `append_header` は複数値ヘッダー
- `with_cookie` で response cookie を追加

```rust
use runbridge::{Cookie, Response};

let response = Response::ok()
    .append_header("Vary", "Accept-Encoding")
    .append_header("Vary", "Origin")
    .with_cookie(Cookie::new("session", "abc123").with_path("/").http_only(true));
```

## Middleware

middleware は request を受け取り、必要なら `next.run(req).await` を呼びます。early return と response 加工の両方を同じ middleware で扱えます。

```rust
use async_trait::async_trait;
use runbridge::{Middleware, Next, Request, Response};
use runbridge::error::Error;

struct AuthMiddleware;

#[async_trait]
impl Middleware for AuthMiddleware {
    async fn handle(&self, req: Request, next: Next<'_>) -> Result<Response, Error> {
        if req.headers.get("x-auth-token") != Some("secret-token") {
            return Ok(
                Response::unauthorized()
                    .with_header("Content-Type", "text/plain")
                    .with_body("Unauthorized".as_bytes().to_vec())
            );
        }

        let mut response = next.run(req).await?;
        response.headers.append("X-Auth-Checked", "true");
        Ok(response)
    }
}
```

## App State

```rust
use std::sync::Arc;
use runbridge::{handler, Request, Response, RunBridge};

#[derive(Clone)]
struct AppState {
    prefix: String,
}

let app = RunBridge::builder()
    .state(Arc::new(AppState {
        prefix: "v1".to_string(),
    }))
    .handler(handler::get("/health", |_req: Request| {
        Ok(Response::ok().with_body("ok".as_bytes().to_vec()))
    }))
    .build();
```

`State<T>` extractor trait も公開されています。P0 では既存の `Request` / `Request + Json<T>` builders を互換レイヤとして残しつつ、extractor を使う core に移行しています。

## Fallback

```rust
use runbridge::{handler, Request, Response, RunBridge};

let app = RunBridge::builder()
    .handler(handler::get("/hello", |_req: Request| Ok("hello")))
    .fallback(handler::fallback(|req: Request| {
        Ok(Response::not_found().with_body(format!("missing: {}", req.path).into_bytes()))
    }))
    .build();
```

fallback は `NotFound` のときだけ動きます。`405 Method Not Allowed` には使われません。

## Extractors

公開 trait:

- `FromRequestParts`
- `FromRequest`
- `IntoResponse`

公開 concrete extractor:

- `Path<T>`
- `Query<T>`
- `Json<T>`
- `State<T>`

P0 時点では variadic extractor builder はまだ導入していません。既存 builder は内部で extractor core を使う互換レイヤです。

## Migration Note

P0 以前からの主な breaking changes:

1. `req.query_params` は `req.query` に変わりました
2. `HashMap<String, String>` header/query 前提は使えません
3. regex route (`"^/items$"`) は廃止です
4. `Middleware::pre_process/post_process` は廃止です
5. `RunBridge::find_handler()` は廃止です
6. response cookie は `headers["Set-Cookie"]` ではなく `response.cookies` です

## 開発コマンド

```bash
cargo test
cargo test --features cloud_run
cargo test --features cgi
cargo test --features lambda
```
