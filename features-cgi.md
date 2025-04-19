# CGI対応機能拡張 (feature: cgi)

以下では、従来のLambda / Cloud Run向け実装に加えて、CGI環境で動作させるための拡張要件および実装ガイドラインを定義します。

## 1. Cargo.toml設定
- [features] セクションに `cgi` フィーチャーを追加
- オプション依存として、マクロやユーティリティ用に `cgi` クレートを指定

```toml
[features]
default = []
cgi = []

[dependencies]
# 共通依存（例: tokio, serde, log）
# CGI専用依存
cgi = { version = "*", optional = true }
```

## 2. モジュール構成
- `src/cgi.rs` を新規作成し、以下を実装
  - 環境変数と標準入力から `Request` を構築する `fn run_cgi()`
  - ビルド時に `#[cfg(feature = "cgi")]` で有効化
- `lib.rs` またはバイナリターゲットで、`main_cgi` 関数をエントリポイントとしてエクスポート

## 3. リクエスト変換
- **環境変数**
  - `REQUEST_METHOD`、`PATH_INFO`、`QUERY_STRING`、`CONTENT_TYPE`、`CONTENT_LENGTH`
  - `HTTP_*` プレフィックスのヘッダーを解析
- **ボディ**
  - `CONTENT_LENGTH` を参照し、`stdin` からバイト列を読み込む
- 共通の抽象 `Request` 型へマッピング

## 4. レスポンス出力
- `Response` オブジェクトを受け取り、以下を標準出力に書き出す
  1. ステータス行：`Status: {status_code} {reason_phrase}`
  2. 各ヘッダー行：`{Header-Name}: {value}`
  3. 空行
  4. ボディ

## 5. ロギングとエラーハンドリング
- CGIでは標準出力をHTTP出力に使用するため、ログは標準エラー (`stderr`) へ出力
- `log` クレートの設定を見直し、`stderr` 出力用のログターゲットを追加
- エラー発生時は適切なステータスコード（500など）およびエラーメッセージをHTTPレスポンス形式で返却

## 6. バイナリターゲットとビルド設定
- `Cargo.toml` に以下のようにバイナリセクションを追加

```toml
[[bin]]
name = "myapi-cgi"
path = "src/cgi_main.rs"
required-features = ["cgi"]
```
- `src/cgi_main.rs` を作成し、`cgi::run_cgi()` を呼び出すエントリポイントを実装

## 7. テストとドキュメント
- **ユニットテスト**: 環境変数と標準入力からの `Request` 生成処理を分離した関数に対してテストを記載
- **統合テスト**: シェルスクリプト／CI上で実際のCGIライクな呼び出しを行い、期待するHTTPヘッダー・ボディを検証
- READMEへ使用例（シェルからの呼び出しサンプル）を追記

## 8. 制限／注意事項
- CGIプロセスの起動コストが高いため、リクエストごとにプロセスが生成されることを考慮
- Keep-Alive非対応、リクエストライフサイクルは1回限り
- 大きなリクエストボディへの対応は `CONTENT_LENGTH` の上限値を設けることを推奨 