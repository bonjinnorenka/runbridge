#!/bin/bash

# CGI テストスクリプト - hello_world サンプル用
# このスクリプトはRunBridge CGI実装のhello_worldサンプルをテストします

# スクリプトの設定
set -e  # エラー時に停止
BINARY_PATH="./target/release/runbridge-hello-world"
SCRIPT_DIR=$(cd "$(dirname "$0")" && pwd)

# カラー設定
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# CGIバイナリをビルド
echo -e "${YELLOW}CGIバイナリをビルドしています...${NC}"
cargo build --release --features cgi --no-default-features
if [ ! -f "$BINARY_PATH" ]; then
    echo -e "${RED}エラー: ビルド後も $BINARY_PATH が見つかりません${NC}"
    exit 1
fi
echo -e "${GREEN}ビルド完了！${NC}"

# テスト結果カウンター
TOTAL_TESTS=0
PASSED_TESTS=0

# テスト実行関数
run_test() {
    local test_name="$1"
    local request_method="$2"
    local request_uri="$3"
    local query_string="$4"
    local expected_message_pattern="$5"
    local expected_status="$6"
    
    TOTAL_TESTS=$((TOTAL_TESTS + 1))
    echo -e "${YELLOW}テスト実行中: $test_name${NC}"
    
    # CGI環境変数を設定
    export REQUEST_METHOD="$request_method"
    export PATH_INFO="$request_uri"
    export REQUEST_URI="$request_uri"
    export QUERY_STRING="$query_string"
    export CONTENT_TYPE=""
    export CONTENT_LENGTH="0"
    export SERVER_NAME="localhost"
    export SERVER_PORT="80"
    export HTTP_HOST="localhost"
    
    # CGIバイナリを実行してレスポンスを取得（標準出力のみキャプチャ）
    local output
    output=$("$BINARY_PATH" 2>/dev/null)
    local exit_code=$?
    
    # レスポンスを解析（CRLF対応）
    local status_line=$(echo "$output" | head -1 | tr -d '\r')
    local headers_end_line=$(echo "$output" | grep -n "^$\|^[[:space:]]*$" | head -1 | cut -d: -f1)
    local body=""
    
    if [ -n "$headers_end_line" ]; then
        body=$(echo "$output" | tail -n +"$((headers_end_line + 1))" | tr -d '\r')
        # JSONレスポンスの場合、messageフィールドを抽出
        if [[ "$body" == \{* ]]; then
            # JSON形式の場合、messageフィールドの値を抽出
            body=$(echo "$body" | sed -n 's/.*"message"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p')
        fi
    fi
    
    echo "  リクエスト: $request_method $request_uri?$query_string"
    echo "  ステータス行: $status_line"
    echo "  レスポンスボディ: $body"
    
    # ステータスコードの確認
    local status_ok=false
    if [[ "$status_line" == *"$expected_status"* ]]; then
        status_ok=true
    fi
    
    # メッセージパターンの確認
    local message_ok=false
    if [[ "$body" == *"$expected_message_pattern"* ]]; then
        message_ok=true
    fi
    
    # テスト結果の判定
    if [ "$status_ok" = true ] && [ "$message_ok" = true ]; then
        echo -e "  ${GREEN}✓ PASS${NC}"
        PASSED_TESTS=$((PASSED_TESTS + 1))
    else
        echo -e "  ${RED}✗ FAIL${NC}"
        if [ "$status_ok" = false ]; then
            echo -e "    ${RED}期待されたステータス「$expected_status」が見つかりません${NC}"
        fi
        if [ "$message_ok" = false ]; then
            echo -e "    ${RED}期待されたメッセージパターン「$expected_message_pattern」が見つかりません${NC}"
        fi
    fi
    echo
    
    # 環境変数をクリア
    unset REQUEST_METHOD PATH_INFO REQUEST_URI QUERY_STRING CONTENT_TYPE CONTENT_LENGTH
    unset SERVER_NAME SERVER_PORT HTTP_HOST
}

echo -e "${YELLOW}===========================================${NC}"
echo -e "${YELLOW}RunBridge CGI Hello World テストスイート${NC}"
echo -e "${YELLOW}===========================================${NC}"
echo

# テストケース1: 基本的な /hello リクエスト
run_test \
    "基本的なhelloリクエスト" \
    "GET" \
    "/hello" \
    "" \
    "Hello, World!" \
    "200 OK"

# テストケース2: name パラメータ付きリクエスト
run_test \
    "nameパラメータ付きリクエスト" \
    "GET" \
    "/hello" \
    "name=Alice" \
    "Hello, Alice!" \
    "200 OK"

# テストケース3: 日本語言語指定リクエスト
run_test \
    "日本語言語指定リクエスト" \
    "GET" \
    "/hello" \
    "lang=ja" \
    "こんにちは、World!" \
    "200 OK"

# テストケース4: フランス語言語指定リクエスト
run_test \
    "フランス語言語指定リクエスト" \
    "GET" \
    "/hello" \
    "lang=fr" \
    "Bonjour, World !" \
    "200 OK"

# テストケース5: スペイン語言語指定リクエスト
run_test \
    "スペイン語言語指定リクエスト" \
    "GET" \
    "/hello" \
    "lang=es" \
    "¡Hola, World!" \
    "200 OK"

# テストケース6: ドイツ語言語指定リクエスト
run_test \
    "ドイツ語言語指定リクエスト" \
    "GET" \
    "/hello" \
    "lang=de" \
    "Hallo, World!" \
    "200 OK"

# テストケース7: name と lang の両方を指定（URLエンコードあり）
run_test \
    "name+lang両方指定リクエスト（URLデコード）" \
    "GET" \
    "/hello" \
    "name=%E5%A4%AA%E9%83%8E&lang=ja" \
    "こんにちは、太郎!" \
    "200 OK"

# テストケース8: 未知の言語指定（デフォルトの英語になる）
run_test \
    "未知の言語指定リクエスト" \
    "GET" \
    "/hello" \
    "name=Bob&lang=unknown" \
    "Hello, Bob!" \
    "200 OK"

# テストケース9: 存在しないパスへのリクエスト（404エラー期待）
run_test \
    "存在しないパスへのリクエスト" \
    "GET" \
    "/nonexistent" \
    "" \
    "Not Found" \
    "404"

# 結果サマリーを表示
echo -e "${YELLOW}===========================================${NC}"
echo -e "${YELLOW}テスト結果サマリー${NC}"
echo -e "${YELLOW}===========================================${NC}"
echo -e "実行したテスト数: ${TOTAL_TESTS}"
echo -e "成功したテスト数: ${PASSED_TESTS}"
echo -e "失敗したテスト数: $((TOTAL_TESTS - PASSED_TESTS))"

if [ $PASSED_TESTS -eq $TOTAL_TESTS ]; then
    echo -e "${GREEN}すべてのテストが成功しました！${NC}"
    exit 0
else
    echo -e "${RED}$((TOTAL_TESTS - PASSED_TESTS))個のテストが失敗しました${NC}"
    exit 1
fi