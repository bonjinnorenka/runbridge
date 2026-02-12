# Repository Guidelines

## プロジェクト構成 / モジュール
- ルート: `Cargo.toml`（features: `lambda` / `cloud_run` / `cgi`）。
- ライブラリ: `src/lib.rs`（公開APIとfeature排他の検証）。
- 共通/基盤: `src/common/`（Request/Response/Middleware等）, `src/handler.rs`, `src/error.rs`。
- 実行ターゲット: `src/lambda.rs`, `src/cloudrun.rs`, `src/cgi.rs`。
- バイナリ: `src/main.rs` → `bootstrap`、`src/cgi_main.rs` → `runbridge-cgi`。
- テスト: `tests/`（統合/CGIテスト）。
- サンプル: `examples/`（単発ファイル）, `example/`（サブクレート）。

## ビルド・実行・開発コマンド
- 開発サーバ（Cloud Run相当）: `cargo run --features cloud_run`
- Lambda用ビルド: `cargo build --release --features lambda`
- CGI用ビルド: `cargo build --release --features cgi --bin runbridge-cgi`
- テスト一式: `cargo test`（必要に応じて `--features cgi`）
- ドキュメント: `cargo doc --no-deps`（CIはPagesへ公開）
- ログ有効化例: `RUST_LOG=info cargo run --features cloud_run`

## コーディング規約 / 命名
- フォーマット: `cargo fmt --all`（4スペース, Rust標準）。
- 静的解析: `cargo clippy --all-features -D warnings`。
- 命名: モジュール/関数は`snake_case`、型は`PascalCase`、定数は`SCREAMING_SNAKE_CASE`、featureは`snake_case`。

## テスト方針
- フレームワーク: `tokio`（`#[tokio::test]`）。
- 配置: 単体は各モジュール、統合は`tests/`。CGIは`#![cfg(feature = "cgi")]`で切替。
- 実行例: `cargo test --features cloud_run`／`cargo test --features cgi`。
- カバレッジ: CIは`cargo tarpaulin --all-features`を使用。ローカルでも同等実行可。

## コミット / PR ガイド
- メッセージ: 可能ならConventional Commits（例: `feat: ...`, `fix: ...`）。短い要約 + 必要な詳細（日本語可）。
- PR要件: 目的/背景、対象feature（`lambda`/`cloud_run`/`cgi`）、動作確認手順、テスト追加/影響範囲、関連Issue、ログ/レスポンスのスクリーンショットがあれば添付。
- CIが緑になることを前提（ビルド/テスト/ドキュメント/サンプル検証）。

## セキュリティ / 設定の注意
- featureは相互排他（`src/lib.rs`で競合をコンパイルエラー化）。同時に複数を有効にしない。
- CGI実行時は環境変数（`REQUEST_METHOD`, `PATH_INFO`, `QUERY_STRING`, `CONTENT_TYPE`, `CONTENT_LENGTH`, `HTTP_*`）を適切に設定。
- ログは`env_logger`。本番で冗長ログを避ける設定（`RUST_LOG=warn`など）を推奨。
