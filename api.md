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
- `handler::patch("/users/{id}", ...)`
- `handler::head("/users/{id}", ...)`
- `handler::route(Method::PATCH, "/users/{id}", custom_handler)`
- `Router::new().route(...).merge(...).nest("/api", ...)`

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

## Fixed file helper

### Public surface

- `RunBridgeBuilder::try_fixed_file(path, file_path)`
- `RunBridgeBuilder::fixed_file(path, file_path)`
- `RunBridgeBuilder::try_fixed_file_with(path, file_path, FixedFileOptions)`
- `RunBridgeBuilder::fixed_file_with(path, file_path, FixedFileOptions)`
- `FixedFileOptions::new().content_type(...).cache_control(...).content_disposition(...).header(...).without_head()`

### Behavior

- 固定ファイルは builder 実行時に読み込んで `Bytes` として保持する
- `GET` は必ず登録する
- `HEAD` はデフォルトで同時登録し、body 抑止は `RunBridge::handle_request()` の既存処理に任せる
- `Content-Type` は拡張子から自動推定し、未判定時は `application/octet-stream`
- `content_type(...)` 指定時は自動推定より優先する
- `Cache-Control` / `Content-Disposition` / 任意ヘッダーを追加できる

### Constraints

- route path は固定静的パスのみ対応し、`{param}` を含む path template は拒否する
- `extra_headers` で `Content-Type` / `Cache-Control` / `Content-Disposition` / `Content-Length` の上書きは許可しない
- directory serving、dynamic file resolution、range request、conditional GET、streaming は対象外

### Router flatten

- `Router` は tree を保持せず flatten 済み `Route` 列へ落とす
- `nest("/api", router)` は prefix を path template として単純結合する
- duplicate route 検出は flatten 後の `(method, path)` に対して行う
- `fallback` と `state` は app-wide only

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

### P1 concrete extractors

- `Form<T>`: `application/x-www-form-urlencoded` のみ受理し、`serde_html_form` で deserialize
- `TextBody`: body を UTF-8 として読む
- `BytesBody`: body を `Bytes` のまま返す

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

### P1 middleware order

- `RunBridgeBuilder.middleware()` が最外周
- `Router.middleware()` はその内側
- `Route::layer()` は route ごとの最内周
- response の復路は逆順

## App state

- `RunBridge::builder().state(Arc<T>)`
- `State<T>` extractor で取得
- 複数依存は 1 つの state struct に束ねる前提

## Error handling

- default は `Response::from_error()` と同じ固定マッピング
- `RunBridgeBuilder::error_handler(...)` で app ごとに差し替え可能
- custom error handler は handler / middleware / fallback handler 自体が返した `Err(Error)` のみ対象
- `404` / `405` の router outcome には適用しない

## CORS helper

- `Cors` は `Middleware` 実装
- actual request では許可 origin のときのみ `Access-Control-Allow-Origin` などを付与する
- preflight request (`OPTIONS + Origin + Access-Control-Request-Method`) は middleware が `204 No Content` で short-circuit する
- `allow_headers` 未指定時に request header を反射しない
- `allow_any_origin() + allow_credentials(true)` は `try_validate()` で `ConfigurationError`

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
