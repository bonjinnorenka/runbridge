//! CGI環境での実行をサポートするモジュール
//!
//! 環境変数と標準入力からリクエストを構築し、
//! 標準出力にHTTPレスポンスフォーマットで出力するための機能を提供します。

use std::collections::HashMap;
use std::env;
use std::io::{self, Read, Write};
use log::{debug, error, info};
use std::fs::OpenOptions;
use chrono::Local;
use tokio::task;

use crate::common::{Method, Request, Response, parse_query_string};
use crate::error::Error;
use crate::RunBridge;

/// CGIリクエスト情報をRunBridgeリクエストに変換し、処理を実行する
pub async fn run_cgi(app: RunBridge) -> Result<(), Error> {
    // 環境変数からリクエスト情報を取得
    let method_str = env::var("REQUEST_METHOD").map_err(|_| {
        Error::InvalidRequestBody("REQUEST_METHOD environment variable not set".to_string())
    })?;
    
    let method = Method::from_str(&method_str).ok_or_else(|| {
        Error::InvalidRequestBody(format!("Invalid HTTP method: {}", method_str))
    })?;
    
    let path = env::var("PATH_INFO").unwrap_or_else(|_| "/".to_string());
    let query_string = env::var("QUERY_STRING").unwrap_or_default();
    
    // クエリパラメータを解析
    let query_params = parse_query_string(&query_string);
    
    // ヘッダーを取得
    let headers = get_cgi_headers();
    
    // ボディを読み込む
    let body = read_request_body()?;
    
    // リクエストを構築
    let mut request = Request::new(method, path.clone());
    request.query_params = query_params;
    request.headers = headers;
    request.body = body;
    
    // リクエストを処理
    debug!("Processing CGI request: {} {}", method, path);
    
    // ハンドラ内でのpanicを検知するためにspawnしてJoinErrorを検査
    let task_result = task::spawn(async move {
        process_request(app, request).await
    }).await;

    let response = match task_result {
        // タスクが正常終了し、かつハンドラがResult::Ok/Errを返した場合
        Ok(inner_result) => match inner_result {
            Ok(res) => res,
            Err(err) => {
                error!("Error processing request: {:?}", err);
                log_error_to_file(&format!("Handler returned error at {} {}: {:?}", method, path, err));
                match err {
                    Error::RouteNotFound(msg) => {
                        Response::not_found()
                            .with_header("Content-Type", "text/plain")
                            .with_body(format!("Not Found: {}", msg).into_bytes())
                    }
                    _ => Response::internal_server_error()
                        .with_header("Content-Type", "text/plain")
                        .with_body(format!("Internal Server Error: {}", err).into_bytes())
                }
            }
        },
        // タスクがpanicした場合
        Err(join_err) => {
            let panic_info = if join_err.is_panic() {
                "panic occurred in handler".to_string()
            } else {
                format!("task cancelled: {}", join_err)
            };
            error!("{}", panic_info);
            log_error_to_file(&format!("{} at {} {}", panic_info, method, path));
            Response::internal_server_error()
                .with_header("Content-Type", "text/plain")
                .with_body("Internal Server Error".as_bytes().to_vec())
        }
    };
    
    // レスポンスを標準出力に書き出す
    write_response(response)?;
    
    info!("CGI request processed successfully");
    Ok(())
}

/// 環境変数からHTTPヘッダーを取得する
fn get_cgi_headers() -> HashMap<String, String> {
    let mut headers = HashMap::new();
    for (key, value) in env::vars() {
        let header_name = if key.starts_with("HTTP_") {
            // HTTP_X_AUTH_TOKEN -> X-Auth-Token のように変換
            let header_parts: Vec<&str> = key[5..].split('_').collect();
            let header_name = header_parts.iter()
                .map(|part| {
                    let mut chars = part.chars();
                    match chars.next() {
                        None => String::new(),
                        Some(c) => c.to_ascii_uppercase().to_string() + &chars.as_str().to_ascii_lowercase()
                    }
                })
                .collect::<Vec<String>>()
                .join("-");
            header_name
        } else if key == "CONTENT_TYPE" || key == "CONTENT_LENGTH" {
            let header_parts: Vec<&str> = key.split('_').collect();
            let header_name = header_parts.iter()
                .map(|part| {
                    let mut chars = part.chars();
                    match chars.next() {
                        None => String::new(),
                        Some(c) => c.to_ascii_uppercase().to_string() + &chars.as_str().to_ascii_lowercase()
                    }
                })
                .collect::<Vec<String>>()
                .join("-");
            header_name
        } else {
            continue;
        };
        // ヘッダー名のバリデーション（英数字とハイフンのみ許可、ASCII限定）
        if !is_valid_header_name(&header_name) {
            continue;
        }
        // ヘッダー値のバリデーション（ASCIIホワイトリスト）
        if !is_valid_header_value(&value) {
            continue;
        }
        headers.insert(header_name, value);
    }
    headers
}

/// 最大リクエストボディサイズを取得する
fn get_max_body_size() -> usize {
    const DEFAULT_MAX_SIZE: usize = 5 * 1024 * 1024; // 5MB
    
    env::var("RUNBRIDGE_MAX_BODY_SIZE")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(DEFAULT_MAX_SIZE)
}

/// リクエストボディを標準入力から読み込む
fn read_request_body() -> Result<Option<Vec<u8>>, Error> {
    if let Ok(content_length_str) = env::var("CONTENT_LENGTH") {
        if let Ok(content_length) = content_length_str.parse::<usize>() {
            if content_length > 0 {
                let max_body_size = get_max_body_size();
                if content_length > max_body_size {
                    return Err(Error::PayloadTooLarge(
                        format!(
                            "Request body size {} bytes exceeds maximum allowed size {} bytes",
                            content_length,
                            max_body_size
                        )
                    ));
                }
                
                let mut buffer = vec![0u8; content_length];
                io::stdin().read_exact(&mut buffer).map_err(|e| {
                    Error::InvalidRequestBody(format!("Failed to read request body: {}", e))
                })?;
                return Ok(Some(buffer));
            }
        }
    }
    
    Ok(None)
}

/// リクエストを処理する
async fn process_request(app: RunBridge, request: Request) -> Result<Response, Error> {
    // ハンドラを検索
    let handler = app.find_handler(&request.path, &request.method).ok_or_else(|| {
        Error::RouteNotFound(format!("{} {}", request.method, request.path))
    })?;
    
    // ミドルウェアの前処理を適用
    let mut processed_request = request;
    for middleware in app.middlewares() {
        processed_request = middleware.pre_process(processed_request).await?;
    }
    
    // ハンドラでリクエストを処理
    let handler_result = handler.handle(processed_request).await;
    
    // レスポンスの処理
    let mut response = match handler_result {
        Ok(res) => res,
        Err(e) => {
            error!("Handler error: {}", e);
            return Ok(Response::from_error(&e));
        }
    };
    
    // ミドルウェアの後処理を適用
    for middleware in app.middlewares() {
        match middleware.post_process(response).await {
            Ok(processed) => response = processed,
            Err(e) => {
                error!("Middleware error in post-processing: {}", e);
                response = Response::from_error(&e);
            }
        }
    }
    
    Ok(response)
}

/// ヘッダー名が安全かどうか検証する（ASCII英数+ハイフンのみ）
fn is_valid_header_name(name: &str) -> bool {
    let b = name.as_bytes();
    if b.is_empty() {
        return false;
    }
    // 許可: A-Z a-z 0-9 '-'
    if !b.iter().all(|&c| c.is_ascii_alphanumeric() || c == b'-') {
        return false;
    }
    true
}

/// ヘッダー値が安全かどうか検証する（ASCIIのホワイトリスト）
/// 許可: HTAB(0x09), SP(0x20), 可視ASCII(0x21–0x7E)
fn is_valid_header_value(value: &str) -> bool {
    value
        .as_bytes()
        .iter()
        .all(|&c| c == b'\t' || c == b' ' || (0x21..=0x7e).contains(&c))
}

/// レスポンスを標準出力に書き出す
fn write_response(mut response: Response) -> Result<(), Error> {
    // 出力前に全ヘッダーを検証し、予約ヘッダーを除外する
    let mut sanitized_headers: Vec<(String, String)> = Vec::new();

    for (name, value) in &response.headers {
        // 予約ヘッダーはユーザー指定を無視
        if name.eq_ignore_ascii_case("Status") || name.eq_ignore_ascii_case("Content-Length") {
            continue;
        }
        if !is_valid_header_name(name) || !is_valid_header_value(value) {
            error!("Invalid header detected - name: '{}', value: '{}'", name, value);
            log_error_to_file(&format!(
                "CRLF injection attempt detected in header: '{}': '{}'",
                name, value
            ));
            // 安全な400レスポンスを構築
            response = Response::new(400)
                .with_header("Content-Type", "text/plain; charset=utf-8")
                .with_body(b"Bad Request: Invalid header".to_vec());
            sanitized_headers.clear();
            break;
        }
        sanitized_headers.push((name.clone(), value.clone()));
    }

    // ステータスコードとReason Phraseを準備
    let reason_phrase = match response.status {
        200 => "OK",
        201 => "Created",
        204 => "No Content",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        413 => "Payload Too Large",
        500 => "Internal Server Error",
        _ => "Unknown",
    };

    let mut out = io::stdout().lock();
    // ステータス行（CRLF）
    out.write_all(format!("Status: {} {}\r\n", response.status, reason_phrase).as_bytes())
        .map_err(|e| Error::InternalServerError(format!("Failed to write status line: {}", e)))?;

    // 最終的なヘッダー集合（予約ヘッダーは除外されたもの）
    for (name, value) in sanitized_headers {
        out.write_all(format!("{}: {}\r\n", name, value).as_bytes()).map_err(|e| {
            Error::InternalServerError(format!("Failed to write header: {}", e))
        })?;
    }

    // Content-Length をフレームワーク側で付与（ボディがある場合）
    if let Some(body) = &response.body {
        out.write_all(format!("Content-Length: {}\r\n", body.len()).as_bytes()).map_err(|e| {
            Error::InternalServerError(format!("Failed to write Content-Length: {}", e))
        })?;
    }

    // ヘッダーとボディの区切り（CRLF）
    out.write_all(b"\r\n").map_err(|e| {
        Error::InternalServerError(format!("Failed to write header/body separator: {}", e))
    })?;

    // ボディ出力
    if let Some(body) = response.body {
        out.write_all(&body).map_err(|e| {
            Error::InternalServerError(format!("Failed to write response body: {}", e))
        })?;
    }

    Ok(())
}

/// エラー内容をログファイルに追記する
fn log_error_to_file(message: &str) {
    let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S");
    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open("runbridge_error.log")
    {
        let _ = writeln!(file, "[{}] {}", timestamp, message);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_parse_query_string() {
        let query = "name=John&age=30&city=Tokyo";
        let params = parse_query_string(query);
        
        assert_eq!(params.get("name"), Some(&"John".to_string()));
        assert_eq!(params.get("age"), Some(&"30".to_string()));
        assert_eq!(params.get("city"), Some(&"Tokyo".to_string()));
    }

    #[test]
    fn test_parse_query_string_url_encoding() {
        // URLエンコードされたクエリ文字列
        let query = "name=%E3%81%82%E3%81%84%E3%81%86%E3%81%88%E3%81%8A&city=Tokyo%20Station&lang=ja%2Den";
        let params = parse_query_string(query);

        // "あいうえお"（UTF-8でURLエンコード）
        assert_eq!(params.get("name"), Some(&"あいうえお".to_string()));
        // スペースが%20でエンコードされている
        assert_eq!(params.get("city"), Some(&"Tokyo Station".to_string()));
        // ハイフンが%2Dでエンコードされている
        assert_eq!(params.get("lang"), Some(&"ja-en".to_string()));
    }
    
    #[test]
    fn test_get_cgi_headers() {
        // 環境変数を設定 (テスト専用の環境変数を使用すべき)
        use temp_env::with_vars;
        with_vars([
            ("HTTP_CONTENT_TYPE", Some("application/json")),
            ("HTTP_X_CUSTOM_HEADER", Some("test value")),
            ("HTTP_X_AUTH_TOKEN", Some("secret-token")),
            ("CONTENT_LENGTH", Some("123")),
            ("UNRELATED_VAR", Some("should not be included")),
        ], || {
            let headers = get_cgi_headers();
            
            assert_eq!(headers.get("Content-Type"), Some(&"application/json".to_string()));
            assert_eq!(headers.get("X-Custom-Header"), Some(&"test value".to_string()));
            assert_eq!(headers.get("X-Auth-Token"), Some(&"secret-token".to_string()));
            assert_eq!(headers.get("Content-Length"), Some(&"123".to_string()));
            assert_eq!(headers.get("UNRELATED_VAR"), None);
        });
    }
    
    #[test]
    fn test_get_max_body_size_default() {
        use temp_env::with_vars;
        
        // 環境変数が設定されていない場合はデフォルトの5MBを返す
        with_vars([
            ("RUNBRIDGE_MAX_BODY_SIZE", None::<&str>),
        ], || {
            let size = get_max_body_size();
            assert_eq!(size, 5 * 1024 * 1024);
        });
    }
    
    #[test]
    fn test_get_max_body_size_custom() {
        use temp_env::with_vars;
        
        // 環境変数で指定されたサイズを返す
        with_vars([
            ("RUNBRIDGE_MAX_BODY_SIZE", Some("1048576")), // 1MB
        ], || {
            let size = get_max_body_size();
            assert_eq!(size, 1048576);
        });
    }
    
    #[test]
    fn test_get_max_body_size_invalid_env() {
        use temp_env::with_vars;
        
        // 無効な環境変数値の場合はデフォルトにフォールバック
        with_vars([
            ("RUNBRIDGE_MAX_BODY_SIZE", Some("invalid")),
        ], || {
            let size = get_max_body_size();
            assert_eq!(size, 5 * 1024 * 1024);
        });
    }
    
    #[test]
    fn test_is_valid_header_name() {
        // 有効なヘッダー名
        assert!(is_valid_header_name("Content-Type"));
        assert!(is_valid_header_name("X-Custom-Header"));
        assert!(is_valid_header_name("User-Agent"));
        assert!(is_valid_header_name("Accept"));
        
        // 無効なヘッダー名
        assert!(!is_valid_header_name(""));
        assert!(!is_valid_header_name("Content\rType"));
        assert!(!is_valid_header_name("Content\nType"));
        assert!(!is_valid_header_name("Content Type")); // スペース
        assert!(!is_valid_header_name("Content:Type")); // コロン
        assert!(!is_valid_header_name("Content=Type")); // イコール
    }
    
    #[test]
    fn test_is_valid_header_value() {
        // 有効なヘッダー値
        assert!(is_valid_header_value("text/html"));
        assert!(is_valid_header_value("application/json; charset=utf-8"));
        assert!(is_valid_header_value("Bearer token123"));
        assert!(is_valid_header_value("")); // 空文字は有効
        assert!(is_valid_header_value("value\twith\ttab")); // タブは許可
        
        // 無効なヘッダー値（CRLF攻撃）
        assert!(!is_valid_header_value("text/html\r\nSet-Cookie: malicious"));
        assert!(!is_valid_header_value("text/html\nX-Evil: attack"));
        assert!(!is_valid_header_value("text/html\rX-Evil: attack"));
        assert!(!is_valid_header_value("value\x00with\x01control")); // 制御文字
    }
} 
