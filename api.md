# RunBridge API設計メモ

## P0 core 方針

P0 では RunBridge の core を以下の責務に分離した。

- `router`: path template の解決、path params 抽出、405 / 404 / fallback 判定
- `endpoint`: `Handler` trait による request -> response 実行
- `middleware chain`: `next` モデルで request / response を包み込む
- `backend adapter`: Lambda / Cloud Run / CGI と core request/response の相互変換

## Core HTTP model

### Request

`Request` は以下を持つ。

- `method: Method`
- `path: String`
- `query: QueryMap`
- `headers: HeaderMap`
- `path_params: HashMap<String, String>`
- `cookies: Vec<Cookie>`
- `body: Option<Bytes>`
- `context: RequestContext`

### Response

`Response` は以下を持つ。

- `status: u16`
- `headers: HeaderMap`
- `cookies: Vec<Cookie>`
- `body: Option<Bytes>`

### HeaderMap

- case-insensitive
- multi-value
- `Set-Cookie` は `Response.cookies` に分離

### QueryMap

- multi-value
- `get()` と `get_all()` の両方を提供
- duplicate key を保持

## Routing

### Public surface

- `handler::get("/users/{id}", ...)`
- `handler::route(Method::PATCH, "/users/{id}", custom_handler)`

### Template rules

- static segment と `{param}` segment のみ対応
- trailing slash は区別する
- regex route はサポートしない

### Resolution rules

- static route > param route
- 同率なら static segment 数が多い方
- 完全同率なら先登録優先

### RouteMatch

- `Matched { route, path_params }`
- `MethodNotAllowed { allow }`
- `NotFound`

`MethodNotAllowed` では `Allow` ヘッダーを返す。fallback は `NotFound` のときだけ使う。

## Handler / Extractor

### Handler

`Handler` は call-only。

```rust
#[async_trait]
pub trait Handler: Send + Sync {
    async fn handle(&self, req: Request) -> Result<Response, Error>;
}
```

route metadata は `Route` が持つ。

### Extractor traits

- `FromRequestParts`
- `FromRequest`
- `IntoResponse`

### P0 concrete extractors

- `Path<T>`
- `Query<T>`
- `Json<T>`
- `State<T>`

`Json<T>` は body-consuming extractor。P0 では既存 builder の `Request + T` 形を互換レイヤとして残し、内部で `Json<T>` を使う。

## Middleware

```rust
#[async_trait]
pub trait Middleware: Send + Sync {
    async fn handle(&self, req: Request, next: Next<'_>) -> Result<Response, Error>;
}
```

- app-level middleware のみ P0 対応
- 登録順の先頭が最外周
- 404 / 405 / fallback / matched route のすべてを包む

## App state

- `RunBridge::builder().state(Arc<T>)`
- `State<T>` extractor で取得
- 複数依存は 1 つの state struct に束ねる前提

## Backend adapter responsibility

### Lambda

- `raw_path`, `raw_query_string`, `cookies`, `path_parameters`, `headers`, `body`
- response cookie は `ApiGatewayV2httpResponse.cookies`

### Cloud Run

- request header は `get_all()` で multi-value を保持
- raw query から `QueryMap` を構築
- response は `append_header` と複数 `Set-Cookie` で返す

### CGI

- env / stdin から `Request` を構築
- response cookie は複数 `Set-Cookie` 行で出力

## P0 migration summary

- `query_params` -> `query`
- regex route -> path template
- `pre_process/post_process` -> `handle(req, next)`
- `find_handler()` -> `resolve()` / `handle_request()`
- `headers["Set-Cookie"]` -> `Response.cookies`
