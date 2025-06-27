# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## 概要

RunBridgeは、単一のコードベースでAWS Lambda、Google Cloud Run、CGI環境すべてに対応するRustサーバーレスAPIフレームワークです。

## コマンド

### ビルド
```bash
# Lambda向け
cargo build --release --features lambda

# Cloud Run向け
cargo build --release --features cloud_run

# CGI向け
cargo build --release --features cgi --bin runbridge-cgi
```

### テスト実行
```bash
# 全テスト実行
cargo test

# インテグレーションテスト
cargo test --test integration_tests

# 特定のテスト実行
cargo test test_handler_matches
```

### 開発用サーバー実行
```bash
# Cloud Run機能でHTTPサーバーとして実行
cargo run --features cloud_run

# Lambdaのローカルテスト用
cargo run --features lambda
```

## アーキテクチャと処理の仕組み

### 1. 統一レイヤー (`src/common/mod.rs`)
- **Request/Response**: 各プラットフォーム固有のHTTP形式を統一的な内部形式に変換
- **Handler trait**: リクエスト処理の抽象化インターフェース
- **Middleware trait**: 前処理・後処理を行うミドルウェアシステム
- **Method enum**: HTTPメソッドの統一表現

### 2. ハンドラーシステム (`src/handler.rs`)
- **RouteHandler**: 同期処理用のルートハンドラー実装
- **AsyncRouteHandler**: 非同期処理用のルートハンドラー実装
- **ResponseWrapper trait**: 任意の型をHTTPレスポンスに変換
- **パターンマッチング**: 正規表現によるパス照合（例: `^/items/\d+$`）
- **ハンドラー登録順序**: パス内の`/`の数で降順ソートしてネストが深いパスを優先処理

### 3. プラットフォーム別実装
#### Lambda (`src/lambda.rs`)
- API Gateway v2 HTTP形式とRunBridge内部形式の相互変換
- Base64エンコーディング対応
```rust
convert_apigw_request() -> Request
convert_to_apigw_response(Response) -> ApiGatewayV2httpResponse
```

#### Cloud Run (`src/cloudrun.rs`)
- actix-webベースのHTTPサーバー実装
- 汎用ルーティング（`/{path:.*}`）ですべてのリクエストをキャッチ
```rust
convert_request() -> Request
convert_to_http_response(Response) -> HttpResponse
```

#### CGI (`src/cgi.rs`)
- 環境変数からHTTPリクエスト情報を取得
- 標準入力からボディ読み込み、標準出力にレスポンス書き出し
- パニック検知機能付き（`tokio::task::spawn`でJoinErrorを監視）
- エラーログファイル出力（`runbridge_error.log`）

### 4. リクエスト処理フロー
1. プラットフォーム固有形式から`Request`に変換
2. パスパターンによるハンドラー検索（深いパス優先）
3. ミドルウェア前処理適用
4. ハンドラー実行
5. ミドルウェア後処理適用
6. `Response`からプラットフォーム固有形式に変換

### 5. エラーハンドリング (`src/error.rs`)
- 統一エラー型でHTTPステータスコードへの自動マッピング
- プラットフォーム固有のエラー処理を抽象化

## 開発パターン

### ハンドラー作成
```rust
// 同期GET
fn get_handler(req: Request) -> Result<ResponseType, Error> { /* */ }
let handler = handler::get("/path", get_handler);

// 非同期POST
async fn post_handler(req: Request, body: RequestType) -> Result<ResponseType, Error> { /* */ }
let handler = handler::async_post("/path", post_handler);
```

### ミドルウェア作成
```rust
#[async_trait]
impl Middleware for CustomMiddleware {
    async fn pre_process(&self, req: Request) -> Result<Request, Error> { /* */ }
    async fn post_process(&self, res: Response) -> Result<Response, Error> { /* */ }
}
```

### アプリケーション構築
```rust
let app = RunBridge::builder()
    .middleware(middleware)
    .handler(handler)
    .build();
```

## 重要な注意点

- **パスパターン**: 正規表現を使用（例: `r"^/items/\d+$"`）
- **ハンドラー順序**: 深いパス（`/`が多い）が優先されるよう自動ソート
- **Feature フラグ**: lambda/cloud_run/cgiは排他的に使用
- **CGI環境**: パニック時のエラーログファイル出力機能あり
- **非同期対応**: tokioランタイム使用、同期・非同期ハンドラー両対応