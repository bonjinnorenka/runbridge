# RunBridge Hello World Async Example

これは非同期処理を行うRunBridgeのサンプルアプリケーションです。このアプリケーションはベクター内にstructを含むレスポンスを返すエンドポイントも提供しています。

## 概要

このサンプルは以下の機能を実装しています：

1. 指定された名前と言語に基づいて挨拶文を返す非同期API
2. アイテムのリストをJSON形式でベクターとして返す非同期API
3. 文字列のリストをストラクチャに含めて返す非同期API
4. 文字列のベクターを直接返す非同期API

## APIエンドポイント

### 挨拶エンドポイント

- `GET /hello` - 挨拶を返します

#### クエリパラメータ

- `name` - 挨拶する相手の名前（デフォルト: "World"）
- `lang` - 言語コード（デフォルト: "en"）
  - サポートされている言語: `en`（英語）, `ja`（日本語）, `fr`（フランス語）, `es`（スペイン語）, `de`（ドイツ語）
- `delay` - 応答を遅延させるミリ秒数（デフォルト: 0）

#### レスポンス例

```json
{
  "message": "こんにちは、田中さん!",
  "timestamp": 1648373294,
  "elapsed_ms": 1000
}
```

### アイテムリストエンドポイント

- `GET /items` - アイテムのリストを返します

#### クエリパラメータ

- `count` - 返すアイテムの数（デフォルト: 5）

#### レスポンス例

```json
{
  "items": [
    {
      "id": 1,
      "name": "商品 1",
      "price": 100.0
    },
    {
      "id": 2,
      "name": "商品 2",
      "price": 200.0
    },
    ...
  ],
  "count": 5,
  "timestamp": 1648373294
}
```

### 文字列リストエンドポイント

- `GET /strings` - 文字列のリストをストラクチャに含めて返します

#### クエリパラメータ

- `count` - 返す文字列の数（デフォルト: 5）

#### レスポンス例

```json
{
  "strings": [
    "文字列 1",
    "文字列 2",
    "文字列 3",
    "文字列 4",
    "文字列 5"
  ],
  "count": 5,
  "timestamp": 1648373294
}
```

### 直接文字列ベクターエンドポイント

- `GET /direct-strings` - 文字列のベクターを直接返します

#### クエリパラメータ

- `count` - 返す文字列の数（デフォルト: 5）

#### レスポンス例

```json
[
  "直接ベクター文字列 1",
  "直接ベクター文字列 2",
  "直接ベクター文字列 3",
  "直接ベクター文字列 4",
  "直接ベクター文字列 5"
]
```

## Dockerでのビルドと実行

### ビルド方法

プロジェクトのルートディレクトリから以下のコマンドを実行します：

```bash
# Docker イメージをビルド
docker build -t runbridge-hello-async -f example/helloworld_async/Dockerfile .
```

### 実行方法

```bash
# Docker コンテナを実行（ポート8080を公開）
docker run -p 8080:8080 runbridge-hello-async
```

## curlでのテスト方法

サーバー起動後、以下のcurlコマンドを使用してAPIをテストできます：

### 挨拶エンドポイントのテスト

```bash
# 英語での挨拶
curl "http://localhost:8080/hello?name=John"

# 日本語での挨拶
curl "http://localhost:8080/hello?name=田中&lang=ja" 

# 1秒の遅延付きで挨拶
curl "http://localhost:8080/hello?name=Smith&delay=1000"
```

### アイテムリストエンドポイントのテスト

```bash
# デフォルトの5アイテムを取得
curl "http://localhost:8080/items"

# 10アイテムを取得
curl "http://localhost:8080/items?count=10"

# フォーマットされたJSONを取得
curl "http://localhost:8080/items" | jq
```

### 文字列リストエンドポイントのテスト

```bash
# デフォルトの5文字列を取得
curl "http://localhost:8080/strings"

# 10文字列を取得
curl "http://localhost:8080/strings?count=10"
```

### 直接文字列ベクターエンドポイントのテスト

```bash
# デフォルトの5文字列を取得
curl "http://localhost:8080/direct-strings"

# 10文字列を取得
curl "http://localhost:8080/direct-strings?count=10"
```

## 本番環境へのデプロイ

このサンプルは、Google Cloud RunまたはAWS Lambdaにデプロイすることができます。詳細なデプロイ手順については、メインのドキュメントを参照してください。 