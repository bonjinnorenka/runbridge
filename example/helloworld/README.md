# RunBridge Hello World Example

AWS Lambdaで実行するRunBridgeのHello Worldサンプルアプリケーションです。

## 注意事項

**このサンプルは現在開発中のRunBridgeライブラリを使用しています。ライブラリの実装が完了するまでコンパイルエラーが発生する可能性があります。**

## 概要

このサンプルは、指定された名前と言語に基づいて挨拶文を返すシンプルなAPIを実装しています。

## APIエンドポイント

- `GET /hello` - 挨拶を返します

### クエリパラメータ

- `name` - 挨拶する相手の名前（デフォルト: "World"）
- `lang` - 言語コード（デフォルト: "en"）
  - サポートされている言語: `en`（英語）, `ja`（日本語）, `fr`（フランス語）, `es`（スペイン語）, `de`（ドイツ語）

### レスポンス例

```json
{
  "message": "こんにちは、田中さん！",
  "timestamp": 1648373294
}
```

## ビルドと実行

### ローカルでのビルド

```bash
cargo build
```

### AWS Lambdaへのデプロイ

1. AWS CLIがインストールされていることを確認し、認証情報を設定します。

2. `deploy.sh`スクリプト内の`YOUR_ACCOUNT_ID`を実際のAWSアカウントIDに置き換えます。

3. また、Lambdaの実行ロール`lambda-role`が存在することを確認してください。存在しない場合は作成するか、既存のロールのARNに変更します。

4. deployスクリプトを実行します:

```bash
chmod +x deploy.sh
./deploy.sh
```

5. スクリプト実行後、Lambda関数のURLが表示されます。このURLにアクセスして機能をテストできます。

## 使用例

以下のようなURLで挨拶を試すことができます:

- 英語での挨拶: `<function-url>/hello?name=John`
- 日本語での挨拶: `<function-url>/hello?name=田中&lang=ja`
- フランス語での挨拶: `<function-url>/hello?name=Pierre&lang=fr` 