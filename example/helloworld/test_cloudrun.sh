#!/bin/bash

# Cloud Run テストスクリプト - hello_world サンプル用（Docker使用）
# このスクリプトはRunBridge Cloud Run実装のhello_worldサンプルをテストします

# スクリプトの設定
set -e  # エラー時に停止
DOCKER_IMAGE="runbridge-hello-world:test"
CONTAINER_NAME="runbridge-test-container"
SCRIPT_DIR=$(cd "$(dirname "$0")" && pwd)
TEST_PORT=8080
BASE_URL="http://localhost:${TEST_PORT}"

# カラー設定
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# テスト結果カウンター
TOTAL_TESTS=0
PASSED_TESTS=0

# コンテナクリーンアップ関数
cleanup() {
    echo -e "${BLUE}コンテナをクリーンアップしています...${NC}"
    docker stop "$CONTAINER_NAME" 2>/dev/null || true
    docker rm "$CONTAINER_NAME" 2>/dev/null || true
}

# エラー時のクリーンアップ
trap cleanup EXIT

# Dockerイメージの存在確認
if ! docker image inspect "$DOCKER_IMAGE" >/dev/null 2>&1; then
    echo -e "${RED}エラー: Dockerイメージ '$DOCKER_IMAGE' が見つかりません${NC}"
    echo "まず以下のコマンドでイメージをビルドしてください:"
    echo "cd /home/ryokuryu/runbridge && docker build -t runbridge-hello-world:test -f example/helloworld/Dockerfile ."
    exit 1
fi

# curlコマンドの存在確認
if ! command -v curl &> /dev/null; then
    echo -e "${RED}エラー: curlコマンドが見つかりません${NC}"
    echo "curlをインストールしてください: sudo apt-get install curl"
    exit 1
fi

# テスト実行関数
run_http_test() {
    local test_name="$1"
    local request_method="$2"
    local request_path="$3"
    local expected_status="$4"
    local expected_message_pattern="$5"
    
    TOTAL_TESTS=$((TOTAL_TESTS + 1))
    echo -e "${YELLOW}テスト実行中: $test_name${NC}"
    
    local url="${BASE_URL}${request_path}"
    echo "  リクエスト: $request_method $url"
    
    # curlでHTTPリクエストを送信
    local response
    local http_status
    local response_body
    
    response=$(curl -s -w "HTTP_STATUS:%{http_code}" -X "$request_method" "$url" 2>/dev/null)
    http_status=$(echo "$response" | grep -o "HTTP_STATUS:[0-9]*" | cut -d: -f2)
    response_body=$(echo "$response" | sed 's/HTTP_STATUS:[0-9]*$//')
    
    echo "  HTTPステータス: $http_status"
    echo "  レスポンスボディ: $response_body"
    
    # ステータスコードの確認
    local status_ok=false
    if [[ "$http_status" == "$expected_status" ]]; then
        status_ok=true
    fi
    
    # メッセージパターンの確認
    local message_ok=false
    if [[ "$response_body" == *"$expected_message_pattern"* ]]; then
        message_ok=true
    fi
    
    # テスト結果の判定
    if [ "$status_ok" = true ] && [ "$message_ok" = true ]; then
        echo -e "  ${GREEN}✓ PASS${NC}"
        PASSED_TESTS=$((PASSED_TESTS + 1))
    else
        echo -e "  ${RED}✗ FAIL${NC}"
        if [ "$status_ok" = false ]; then
            echo -e "    ${RED}期待されたステータス「$expected_status」ではなく「$http_status」でした${NC}"
        fi
        if [ "$message_ok" = false ]; then
            echo -e "    ${RED}期待されたメッセージパターン「$expected_message_pattern」が見つかりません${NC}"
        fi
    fi
    echo
}

# サーバー起動待機関数
wait_for_server() {
    local max_attempts=30
    local attempt=0
    
    echo -e "${BLUE}サーバーの起動を待機しています...${NC}"
    
    while [ $attempt -lt $max_attempts ]; do
        if curl -s "$BASE_URL/hello" >/dev/null 2>&1; then
            echo -e "${GREEN}サーバーが起動しました！${NC}"
            return 0
        fi
        
        sleep 1
        attempt=$((attempt + 1))
        echo -n "."
    done
    
    echo -e "\n${RED}エラー: サーバーの起動がタイムアウトしました${NC}"
    return 1
}

echo -e "${YELLOW}===========================================${NC}"
echo -e "${YELLOW}RunBridge Cloud Run Hello World テストスイート${NC}"
echo -e "${YELLOW}===========================================${NC}"
echo

# 既存のコンテナをクリーンアップ
cleanup

# Dockerコンテナを起動
echo -e "${BLUE}Dockerコンテナを起動しています...${NC}"
docker run -d --name "$CONTAINER_NAME" -p "${TEST_PORT}:8080" "$DOCKER_IMAGE"

# サーバーの起動を待機
if ! wait_for_server; then
    echo -e "${RED}サーバーが起動しないため、テストを中止します${NC}"
    exit 1
fi

# テストケース1: 基本的な /hello リクエスト
run_http_test \
    "基本的なhelloリクエスト" \
    "GET" \
    "/hello" \
    "200" \
    "Hello, World!"

# テストケース2: name パラメータ付きリクエスト
run_http_test \
    "nameパラメータ付きリクエスト" \
    "GET" \
    "/hello?name=Alice" \
    "200" \
    "Hello, Alice!"

# テストケース3: 日本語言語指定リクエスト
run_http_test \
    "日本語言語指定リクエスト" \
    "GET" \
    "/hello?lang=ja" \
    "200" \
    "こんにちは、World!"

# テストケース4: フランス語言語指定リクエスト
run_http_test \
    "フランス語言語指定リクエスト" \
    "GET" \
    "/hello?lang=fr" \
    "200" \
    "Bonjour, World !"

# テストケース5: スペイン語言語指定リクエスト
run_http_test \
    "スペイン語言語指定リクエスト" \
    "GET" \
    "/hello?lang=es" \
    "200" \
    "¡Hola, World!"

# テストケース6: ドイツ語言語指定リクエスト
run_http_test \
    "ドイツ語言語指定リクエスト" \
    "GET" \
    "/hello?lang=de" \
    "200" \
    "Hallo, World!"

# テストケース7: name と lang の両方を指定（URLエンコード済み → デコード後）
run_http_test \
    "name+lang両方指定リクエスト（URLデコード）" \
    "GET" \
    "/hello?name=%E5%A4%AA%E9%83%8E&lang=ja" \
    "200" \
    "こんにちは、太郎!"

# テストケース8: 未知の言語指定（デフォルトの英語になる）
run_http_test \
    "未知の言語指定リクエスト" \
    "GET" \
    "/hello?name=Bob&lang=unknown" \
    "200" \
    "Hello, Bob!"

# テストケース9: 存在しないパスへのリクエスト（404エラー期待）
run_http_test \
    "存在しないパスへのリクエスト" \
    "GET" \
    "/nonexistent" \
    "404" \
    "Not Found"

# テストケース10: JSONレスポンスの形式チェック
run_http_test \
    "JSONレスポンス形式チェック" \
    "GET" \
    "/hello?name=JsonTest" \
    "200" \
    '"message":'

# テストケース11: タイムスタンプフィールドの存在確認
run_http_test \
    "タイムスタンプフィールド確認" \
    "GET" \
    "/hello" \
    "200" \
    '"timestamp":'

# コンテナログを表示（デバッグ用）
echo -e "${BLUE}===========================================${NC}"
echo -e "${BLUE}コンテナログ（最後の20行）${NC}"
echo -e "${BLUE}===========================================${NC}"
docker logs --tail=20 "$CONTAINER_NAME"
echo

# 結果サマリーを表示
echo -e "${YELLOW}===========================================${NC}"
echo -e "${YELLOW}テスト結果サマリー${NC}"
echo -e "${YELLOW}===========================================${NC}"
echo -e "実行したテスト数: ${TOTAL_TESTS}"
echo -e "成功したテスト数: ${PASSED_TESTS}"
echo -e "失敗したテスト数: $((TOTAL_TESTS - PASSED_TESTS))"

if [ $PASSED_TESTS -eq $TOTAL_TESTS ]; then
    echo -e "${GREEN}すべてのテストが成功しました！${NC}"
    exit_code=0
else
    echo -e "${RED}$((TOTAL_TESTS - PASSED_TESTS))個のテストが失敗しました${NC}"
    exit_code=1
fi

# クリーンアップはtrapで自動実行される
exit $exit_code