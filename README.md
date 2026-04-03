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
- `Router` と `nest("/prefix", ...)` で route を機能単位に合成できます
- body extractor は `Json<T>` に加えて `Form<T>`, `TextBody`, `BytesBody` を使えます
- `patch()` / `head()` builder と custom `ErrorHandler`, `Cors` middleware を追加しました

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

`Router` を使うと route を prefix 単位でまとめられます。

```rust
use runbridge::{handler, Request, Response, Router, RunBridge};

let api_router = Router::new()
    .route(handler::get("/users", |_req: Request| Ok("users")))
    .route(handler::get("/users/{id}", |req: Request| {
        Ok(Response::ok().with_body(
            req.path_params["id"].clone().into_bytes(),
        ))
    }));

let app = RunBridge::builder()
    .nest("/api", api_router)
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

middleware の実行順は `app-level -> Router.middleware() -> Route::layer() -> endpoint` です。

```rust
use runbridge::{handler, Request, Response, Router, RunBridge};

let api = Router::new()
    .middleware(AuthMiddleware)
    .route(
        handler::get("/profile", |_req: Request| {
            Ok(Response::ok().with_body("ok".as_bytes().to_vec()))
        })
        .layer(AuditMiddleware),
    );

let app = RunBridge::builder()
    .middleware(TracingMiddleware)
    .nest("/api", api)
    .build();
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
- `Form<T>`
- `TextBody`
- `BytesBody`

```rust
use runbridge::{handler, BytesBody, Request, Response, TextBody};
use serde::Deserialize;

#[derive(Deserialize)]
struct LoginForm {
    email: String,
    tags: Vec<String>,
}

let app = RunBridge::builder()
    .handler(handler::post_form("/login", |_req: Request, form: LoginForm| {
        Ok(Response::ok().with_body(form.email.into_bytes()))
    }))
    .handler(handler::post_text("/echo", |_req: Request, text: String| {
        Ok(Response::ok().with_body(text.into_bytes()))
    }))
    .handler(handler::post_bytes("/upload", |_req: Request, body: bytes::Bytes| {
        Ok(Response::ok().with_body(body))
    }))
    .build();
```

`patch()` は `post()` / `put()` と同じ JSON compatibility builder です。`head()` は `get()` と同じシグネチャで登録でき、レスポンス body は core 側で自動抑止されます。

```rust
let app = RunBridge::builder()
    .handler(handler::patch("/items/{id}", |_req: Request, body: serde_json::Value| {
        Ok(body)
    }))
    .handler(handler::head("/health", |_req: Request| Ok(Response::ok())))
    .build();
```

## Fixed Files

少数の固定ファイルを返す専用 helper として `fixed_file()` / `fixed_file_with()` を使えます。ファイルは app build 時に読み込まれ、各 request で再読込しません。

これは汎用 static file server ではありません。directory serving、動的な path-to-file 解決、range request は対象外です。

```rust
use runbridge::{FixedFileOptions, RunBridge};

let app = RunBridge::builder()
    .fixed_file("/favicon.ico", "./public/favicon.ico")
    .fixed_file_with(
        "/robots.txt",
        "./public/robots.txt",
        FixedFileOptions::new()
            .content_type("text/plain; charset=utf-8")
            .cache_control("public, max-age=300"),
    )
    .build();
```

- `GET` は自動登録されます
- `HEAD` もデフォルトで自動登録され、response body は core 側で自動抑止されます
- `FixedFileOptions::without_head()` で `HEAD` 登録を無効化できます
- `Content-Type` は拡張子から自動推定し、`content_type(...)` で上書きできます
- `Cache-Control`、`Content-Disposition`、任意の追加ヘッダーも設定できます

拡張子がない固定ファイル、たとえば `/.well-known/apple-app-site-association` のようなケースでは `content_type(...)` を明示するのが安全です。

## Error Handling

handler や middleware が `Err(Error)` を返したときの response は `ErrorHandler` で差し替えられます。`404` / `405` の static response には適用されません。

```rust
use runbridge::{error_handler, Request, Response, RunBridge, StatusCode};

let app = RunBridge::builder()
    .error_handler(error_handler(|req, err| {
        let path = req.path.clone();
        let status = err.status_code();
        async move {
            Response::with_status(StatusCode::InternalServerError)
                .json(&serde_json::json!({
                    "path": path,
                    "status": status,
                }))
                .unwrap()
        }
    }))
    .build();
```

## CORS

`Cors` は middleware として使います。actual request には `Access-Control-Allow-Origin` などを付与し、preflight request は middleware で `204 No Content` に short-circuit します。

```rust
use runbridge::{Cors, Method, RunBridge};

let cors = Cors::new()
    .allow_origin("https://example.com")
    .allow_methods([Method::GET, Method::PATCH])
    .allow_headers(["Content-Type", "X-Token"])
    .expose_headers(["X-Trace-Id"])
    .max_age(600);

cors.try_validate().unwrap();

let app = RunBridge::builder()
    .middleware(cors)
    .build();
```

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
