#!/bin/bash

# Lambda テストスクリプト - hello_world サンプル用（ローカルRuntimeモック使用）
# このスクリプトは RunBridge Lambda 実装の hello_world サンプルをテストします。

set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "$0")" && pwd)
BIN_PATH="${SCRIPT_DIR}/target/release/runbridge-hello-world"
RUNTIME_HOST="127.0.0.1"
RUNTIME_PORT="9001"
AWS_LAMBDA_RUNTIME_API="${RUNTIME_HOST}:${RUNTIME_PORT}"

# カラー
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

TOTAL_TESTS=0
PASSED_TESTS=0

TEMP_DIR="$(mktemp -d)"
cleanup() {
  echo -e "${BLUE}クリーンアップ中...${NC}"
  if [[ -n "${RUNTIME_PID:-}" ]]; then
    kill "${RUNTIME_PID}" >/dev/null 2>&1 || true
  fi
  rm -rf "${TEMP_DIR}" || true
}
trap cleanup EXIT

echo -e "${YELLOW}===========================================${NC}"
echo -e "${YELLOW}RunBridge Lambda Hello World テストスイート${NC}"
echo -e "${YELLOW}===========================================${NC}"
echo

# 1) ビルド確認（lambda featureのみ有効）
echo -e "${BLUE}バイナリをビルドしています（--no-default-features --features lambda）...${NC}"
(
  cd "${SCRIPT_DIR}" && \
  cargo build --release --no-default-features --features lambda
)

if [[ ! -f "${BIN_PATH}" ]]; then
  echo -e "${RED}エラー: バイナリが見つかりません: ${BIN_PATH}${NC}"
  exit 1
fi

# 2) Lambda Runtime API の簡易モックを起動
#    - GET /2018-06-01/runtime/invocation/next に対して、事前に積んだイベントを順に返却
#    - POST /2018-06-01/runtime/invocation/<id>/response に対するボディを保存

cat >"${TEMP_DIR}/runtime_mock.py" <<'PY'
import json
import os
import threading
import time
from http.server import HTTPServer, BaseHTTPRequestHandler

responses_dir = os.environ["RESP_DIR"]

# 送出するイベント（requestId, event_json文字列）のキュー
events = [
    ("req-1", json.dumps({
        "version": "2.0",
        "routeKey": "$default",
        "rawPath": "/hello",
        "rawQueryString": "",
        "headers": {"host": "localhost"},
        "queryStringParameters": {},
        "requestContext": {"http": {"method": "GET", "path": "/hello"}},
        "pathParameters": {},
        "isBase64Encoded": False,
        "body": None
    })),
    ("req-2", json.dumps({
        "version": "2.0",
        "routeKey": "$default",
        "rawPath": "/hello",
        "rawQueryString": "name=Alice",
        "headers": {"host": "localhost"},
        "queryStringParameters": {"name": "Alice"},
        "requestContext": {"http": {"method": "GET", "path": "/hello"}},
        "pathParameters": {},
        "isBase64Encoded": False,
        "body": None
    })),
    ("req-3", json.dumps({
        "version": "2.0",
        "routeKey": "$default",
        "rawPath": "/hello",
        "rawQueryString": "lang=ja",
        "headers": {"host": "localhost"},
        "queryStringParameters": {"lang": "ja"},
        "requestContext": {"http": {"method": "GET", "path": "/hello"}},
        "pathParameters": {},
        "isBase64Encoded": False,
        "body": None
    })),
    ("req-4", json.dumps({
        "version": "2.0",
        "routeKey": "$default",
        "rawPath": "/hello",
        "rawQueryString": "lang=fr",
        "headers": {"host": "localhost"},
        "queryStringParameters": {"lang": "fr"},
        "requestContext": {"http": {"method": "GET", "path": "/hello"}},
        "pathParameters": {},
        "isBase64Encoded": False,
        "body": None
    })),
    ("req-5", json.dumps({
        "version": "2.0",
        "routeKey": "$default",
        "rawPath": "/hello",
        "rawQueryString": "lang=es",
        "headers": {"host": "localhost"},
        "queryStringParameters": {"lang": "es"},
        "requestContext": {"http": {"method": "GET", "path": "/hello"}},
        "pathParameters": {},
        "isBase64Encoded": False,
        "body": None
    })),
    ("req-6", json.dumps({
        "version": "2.0",
        "routeKey": "$default",
        "rawPath": "/hello",
        "rawQueryString": "lang=de",
        "headers": {"host": "localhost"},
        "queryStringParameters": {"lang": "de"},
        "requestContext": {"http": {"method": "GET", "path": "/hello"}},
        "pathParameters": {},
        "isBase64Encoded": False,
        "body": None
    })),
    ("req-7", json.dumps({
        "version": "2.0",
        "routeKey": "$default",
        "rawPath": "/hello",
        "rawQueryString": "name=%E5%A4%AA%E9%83%8E&lang=ja",
        "headers": {"host": "localhost"},
        "queryStringParameters": {"name": "太郎", "lang": "ja"},
        "requestContext": {"http": {"method": "GET", "path": "/hello"}},
        "pathParameters": {},
        "isBase64Encoded": False,
        "body": None
    })),
    ("req-8", json.dumps({
        "version": "2.0",
        "routeKey": "$default",
        "rawPath": "/hello",
        "rawQueryString": "name=Bob&lang=unknown",
        "headers": {"host": "localhost"},
        "queryStringParameters": {"name": "Bob", "lang": "unknown"},
        "requestContext": {"http": {"method": "GET", "path": "/hello"}},
        "pathParameters": {},
        "isBase64Encoded": False,
        "body": None
    })),
    ("req-9", json.dumps({
        "version": "2.0",
        "routeKey": "$default",
        "rawPath": "/nonexistent",
        "rawQueryString": "",
        "headers": {"host": "localhost"},
        "queryStringParameters": {},
        "requestContext": {"http": {"method": "GET", "path": "/nonexistent"}},
        "pathParameters": {},
        "isBase64Encoded": False,
        "body": None
    })),
]

class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path == "/2018-06-01/runtime/invocation/next":
            if events:
                req_id, payload = events.pop(0)
                body = payload.encode("utf-8")
                self.send_response(200)
                # Lambda-Runtime-Aws-Request-Id ヘッダーが必要
                self.send_header("Lambda-Runtime-Aws-Request-Id", req_id)
                self.send_header("Content-Type", "application/json")
                # 追加必須ヘッダー
                self.send_header("Lambda-Runtime-Deadline-Ms", str(int(time.time()*1000) + 60000))
                self.send_header("Lambda-Runtime-Invoked-Function-Arn", "arn:aws:lambda:us-east-1:123456789012:function:test_fn")
                self.send_header("Lambda-Runtime-Trace-Id", "Root=1-00000000-000000000000000000000000;Parent=0000000000000000;Sampled=0")
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)
            else:
                # イベントが尽きたら長めの待ち（クライアント側でtimeoutさせる）
                self.send_response(204)
                self.end_headers()
        else:
            self.send_response(404)
            self.end_headers()

    def do_POST(self):
        if self.path.startswith("/2018-06-01/runtime/invocation/") and self.path.endswith("/response"):
            length = int(self.headers.get('content-length', 0))
            data = self.rfile.read(length)
            # requestId をパスから抽出
            parts = self.path.split('/')
            req_id = parts[-2]
            with open(os.path.join(responses_dir, f"response_{req_id}.json"), 'wb') as f:
                f.write(data)
            self.send_response(202)
            self.end_headers()
        else:
            self.send_response(404)
            self.end_headers()

def run(host, port):
    httpd = HTTPServer((host, port), Handler)
    httpd.serve_forever()

if __name__ == '__main__':
    host = os.environ.get('HOST', '127.0.0.1')
    port = int(os.environ.get('PORT', '9001'))
    run(host, port)
PY

echo -e "${BLUE}Lambda Runtimeモックを起動します (${RUNTIME_HOST}:${RUNTIME_PORT})...${NC}"
HOST="${RUNTIME_HOST}" PORT="${RUNTIME_PORT}" RESP_DIR="${TEMP_DIR}" \
  python3 "${TEMP_DIR}/runtime_mock.py" >/dev/null 2>&1 &
RUNTIME_PID=$!

sleep 1

# 3) バイナリ実行（一定時間後に終了）
echo -e "${BLUE}Lambda バイナリを起動してイベントを処理します...${NC}"
export AWS_LAMBDA_RUNTIME_API
export RUST_LOG=warn
export AWS_LAMBDA_FUNCTION_NAME=test_fn
export AWS_LAMBDA_FUNCTION_MEMORY_SIZE=128
export AWS_LAMBDA_FUNCTION_VERSION=1
export AWS_REGION=us-east-1
export AWS_EXECUTION_ENV=AWS_Lambda_rust

# timeoutがない環境もあるため、まずコマンド存在チェック
if command -v timeout >/dev/null 2>&1; then
  (cd "${SCRIPT_DIR}" && timeout 8s "${BIN_PATH}") || true
else
  # フォールバック: バックグラウンド実行→数秒待機→kill
  (cd "${SCRIPT_DIR}" && "${BIN_PATH}" & LAMBDA_PID=$!; sleep 8; kill ${LAMBDA_PID} >/dev/null 2>&1 || true)
fi

# 4) 検証関数
run_check() {
  local name="$1"; shift
  local req_id="$1"; shift
  local expect_status="$1"; shift
  local expect_substr="$1"; shift

  TOTAL_TESTS=$((TOTAL_TESTS + 1))
  echo -e "${YELLOW}検証: ${name}${NC}"

  local resp_file="${TEMP_DIR}/response_${req_id}.json"
  if [[ ! -f "${resp_file}" ]]; then
    echo -e "  ${RED}✗ FAIL: レスポンスファイルがありません (${resp_file})${NC}"
    return
  fi

  local content
  content=$(cat "${resp_file}")
  echo "  受信レスポンス: ${content}"

  local ok_status=false
  if echo "${content}" | grep -q '"statusCode"\s*:\s*'"${expect_status}"; then
    ok_status=true
  fi

  local ok_message=false
  if echo "${content}" | grep -q "${expect_substr}"; then
    ok_message=true
  fi

  if [[ "${ok_status}" == true && "${ok_message}" == true ]]; then
    echo -e "  ${GREEN}✓ PASS${NC}"
    PASSED_TESTS=$((PASSED_TESTS + 1))
  else
    echo -e "  ${RED}✗ FAIL${NC}"
    [[ "${ok_status}" == true ]] || echo -e "    ${RED}期待ステータス ${expect_status} が見つかりません${NC}"
    [[ "${ok_message}" == true ]] || echo -e "    ${RED}期待メッセージ '${expect_substr}' が見つかりません${NC}"
  fi
  echo
}

# 5) テストケース
run_check "基本的なhello"            "req-1" 200 "Hello, World!"
run_check "name=Alice"              "req-2" 200 "Hello, Alice!"
run_check "lang=ja"                 "req-3" 200 "こんにちは、World!"
run_check "lang=fr"                 "req-4" 200 "Bonjour, World !"
run_check "lang=es"                 "req-5" 200 "¡Hola, World!"
run_check "lang=de"                 "req-6" 200 "Hallo, World!"
run_check "name=太郎&lang=ja"       "req-7" 200 "こんにちは、太郎!"
run_check "未知の言語（fallback）"    "req-8" 200 "Hello, Bob!"
run_check "存在しないパス 404"       "req-9" 404 "Not Found"

# 6) サマリー
echo -e "${YELLOW}===========================================${NC}"
echo -e "${YELLOW}テスト結果サマリー${NC}"
echo -e "${YELLOW}===========================================${NC}"
echo -e "実行したテスト数: ${TOTAL_TESTS}"
echo -e "成功したテスト数: ${PASSED_TESTS}"
echo -e "失敗したテスト数: $((TOTAL_TESTS - PASSED_TESTS))"

if [[ ${TOTAL_TESTS} -eq ${PASSED_TESTS} ]]; then
  echo -e "${GREEN}すべてのテストが成功しました！${NC}"
  exit 0
else
  echo -e "${RED}$((TOTAL_TESTS - PASSED_TESTS)) 個のテストが失敗しました${NC}"
  exit 1
fi
