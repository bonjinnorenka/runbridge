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

use crate::common::{Method, Request, Response, parse_query_string, get_max_body_size};
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
    
    // ボディを読み込む（上限超過時はここで413レスポンスを返す）
    let body = match read_request_body() {
        Ok(b) => b,
        Err(Error::PayloadTooLarge(_msg)) => {
            let res = Response::new(413)
                .with_header("Content-Type", "text/plain")
                .with_body("Payload Too Large".as_bytes().to_vec());
            write_response(res)?;
            return Ok(());
        }
        Err(e) => return Err(e),
    };
    
    // リクエストを構築
    let mut request = Request::new(method, path.clone());
    request.query_params = query_params;
    // Request取り込み時にヘッダーキーを小文字へ正規化
    request.headers = headers
        .into_iter()
        .map(|(k, v)| (k.to_ascii_lowercase(), v))
        .collect();
    request.body = body;
    
    // gzipボディを解凍（必要な場合のみ）
    if let Err(e) = request.decompress_gzip_body() {
        error!("Failed to decompress gzip body in CGI: {}", e);
        let res = Response::new(400)
            .with_header("Content-Type", "text/plain")
            .with_body(format!("Bad Request: {}", e).as_bytes().to_vec());
        write_response(res)?;
        return Ok(());
    }
    
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
            // panic時は可能な限り具体的な環境情報を追記（センシティブ値はマスク）
            if join_err.is_panic() {
                let ctx = gather_cgi_panic_context(&method.to_string(), &path);
                log_error_to_file(&ctx);
            }
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

/// レスポンスを任意のライターへ書き出す（テスト容易化のため公開しない）
fn write_response_to<W: Write>(mut response: Response, out: &mut W) -> Result<(), Error> {
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

    // ステータス行（CRLF）
    out.write_all(format!("Status: {} {}\r\n", response.status, reason_phrase).as_bytes())
        .map_err(|e| Error::InternalServerError(format!("Failed to write status line: {}", e)))?;

    // Set-Cookie を複数行で正しく出力するために振り分ける
    let mut normal_headers: Vec<(String, String)> = Vec::new();
    let mut set_cookie_values: Vec<String> = Vec::new();

    for (name, value) in sanitized_headers {
        if name.eq_ignore_ascii_case("Set-Cookie") {
            // 複数Cookieが1ヘッダーに連結されていた場合を安全に分割
            let parts = split_set_cookie_header(&value);
            if parts.is_empty() {
                // 分割できない（単一）場合はそのまま扱う
                set_cookie_values.push(value);
            } else {
                set_cookie_values.extend(parts);
            }
        } else {
            normal_headers.push((name, value));
        }
    }

    // 通常ヘッダーを出力
    for (name, value) in normal_headers {
        out.write_all(format!("{}: {}\r\n", name, value).as_bytes()).map_err(|e| {
            Error::InternalServerError(format!("Failed to write header: {}", e))
        })?;
    }

    // Set-Cookie を複数行で出力
    for cookie in set_cookie_values {
        out.write_all(format!("Set-Cookie: {}\r\n", cookie).as_bytes()).map_err(|e| {
            Error::InternalServerError(format!("Failed to write Set-Cookie header: {}", e))
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

/// レスポンスを標準出力に書き出す
fn write_response(response: Response) -> Result<(), Error> {
    let mut out = io::stdout().lock();
    let res = write_response_to(response, &mut out);
    out.flush().map_err(|e| Error::InternalServerError(format!("Failed to flush stdout: {}", e)))?;
    res
}

/// 連結された Set-Cookie ヘッダー値を安全に分割する
/// 注意: RFC的にはSet-Cookieは結合不可だが、実装上HashMap制約の回避として
/// "," 区切りで結合されたケースを考慮し、Expires 属性内のカンマは分割対象から除外する。
fn split_set_cookie_header(value: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut buf = String::new();
    let mut in_expires = false;
    let mut chars = value.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            // セミコロンで属性の区切りを検出（Expires= のスコープ終端にもなる）
            ';' => {
                in_expires = false; // Expires= の属性スコープを抜ける
                buf.push(ch);
            }
            // カンマは、Expires= 属性中ならそのまま、それ以外ならCookie間区切りの可能性
            ',' => {
                if in_expires {
                    buf.push(ch);
                } else {
                    // 直後の空白をスキップ
                    while let Some(' ') = chars.peek() {
                        chars.next();
                    }
                    // 次のトークンが cookie-pair らしい（= を含む）なら分割、それ以外は文字として扱う
                    // 先読みして '=' がセミコロンより前に現れるかを確認
                    let mut lookahead = String::new();
                    let mut iter = chars.clone();
                    let mut seen_eq_before_semicolon = false;
                    while let Some(&c) = iter.peek() {
                        if c == ';' || c == ',' { break; }
                        if c == '=' { seen_eq_before_semicolon = true; break; }
                        lookahead.push(c);
                        iter.next();
                    }
                    if seen_eq_before_semicolon {
                        // ここで一旦Cookieを確定
                        let part = buf.trim();
                        if !part.is_empty() { result.push(part.to_string()); }
                        buf.clear();
                        continue;
                    } else {
                        // Cookie間区切りではないので文字として追加
                        buf.push(',');
                    }
                }
            }
            // 'E' または 'e' から始まる Expires= を検出してフラグを立てる
            'E' | 'e' => {
                // 現在位置から "xpires=" までを確認（ケースインセンシティブ）
                let mut shadow = chars.clone();
                let mut matches = true;
                for expected in ['x','p','i','r','e','s','='] {
                    if let Some(c) = shadow.next() {
                        if c.to_ascii_lowercase() != expected { matches = false; break; }
                    } else { matches = false; break; }
                }
                if matches {
                    in_expires = true;
                }
                buf.push(ch);
            }
            _ => {
                buf.push(ch);
            }
        }
    }

    let tail = buf.trim();
    if !tail.is_empty() {
        result.push(tail.to_string());
    }

    // 単一Cookieしか得られなかった場合は、
    // 呼び出し側でそのまま扱えるように空ベクタではなく単一要素でも返す
    result
}

/// エラー内容をログファイルに追記する
fn log_error_to_file(message: &str) {
    let timestamp = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S%.3f UTC");
    let local_time = Local::now().format("%Y-%m-%d %H:%M:%S%.3f %Z");
    
    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open("runbridge_error.log")
    {
        // より視認性の良いログフォーマット
        let _ = writeln!(file, "================================================================================");
        let _ = writeln!(file, "RUNBRIDGE CGI ERROR");
        let _ = writeln!(file, "Timestamp (UTC): {}", timestamp);
        let _ = writeln!(file, "Timestamp (Local): {}", local_time);
        let _ = writeln!(file, "Process ID: {}", std::process::id());
        let _ = writeln!(file, "--------------------------------------------------------------------------------");
        let _ = writeln!(file, "{}", message);
        let _ = writeln!(file, "================================================================================");
        let _ = writeln!(file);
    }
}

/// panic時に記録するCGI環境の詳細（安全にマスク）を構築
fn gather_cgi_panic_context(method: &str, path: &str) -> String {
    let mut lines = Vec::new();
    lines.push("CGI panic context:".to_string());
    lines.push(format!("  REQUEST_METHOD={}", method));
    lines.push(format!("  PATH_INFO={}", path));

    // 基本的なCGI環境変数
    let basic_vars = [
        "QUERY_STRING",
        "CONTENT_TYPE", 
        "CONTENT_LENGTH",
        "SERVER_PROTOCOL",
        "SERVER_NAME",
        "SERVER_PORT",
        "REMOTE_ADDR",
        "REMOTE_PORT",
    ];

    for key in basic_vars.iter() {
        if let Ok(val) = env::var(key) {
            let v = redact_value_for_log(key, &val);
            lines.push(format!("  {}={}", key, v));
        }
    }

    // 代表的なHTTPヘッダー（存在するもののみ）
    let http_headers = [
        "HTTP_HOST",
        "HTTP_USER_AGENT",
        "HTTP_ACCEPT",
        "HTTP_ACCEPT_ENCODING",
        "HTTP_X_FORWARDED_FOR",
        "HTTP_X_FORWARDED_PROTO",
        "HTTP_X_REQUEST_ID",
        "HTTP_X_AMZN_TRACE_ID",
        "HTTP_AUTHORIZATION",
        "HTTP_COOKIE",
    ];

    lines.push("  HTTP headers:".to_string());
    let mut header_count = 0;
    for key in http_headers.iter() {
        if let Ok(val) = env::var(key) {
            let v = redact_value_for_log(key, &val);
            lines.push(format!("    {}={}", key, v));
            header_count += 1;
        }
    }
    if header_count == 0 {
        lines.push("    (none)".to_string());
    }

    lines.join("\n")
}

fn redact_value_for_log(key: &str, value: &str) -> String {
    let key_l = key.to_ascii_lowercase();
    if key_l == "query_string" {
        return redact_query_string(value);
    }
    if is_sensitive_key_like(&key_l) {
        return "***redacted***".to_string();
    }
    // 長すぎる値は truncate（例：User-Agent）
    if value.len() > 200 {
        format!("{}...[truncated]", &value[..200])
    } else {
        value.to_string()
    }
}

fn is_sensitive_key_like(lower_key: &str) -> bool {
    let patterns = [
        "authorization",
        "cookie",
        "token",
        "secret",
        "password",
        "pass",
        "api-key",
        "api_key",
        "apikey",
        "x-api-key",
        "x_api_key",
        "jwt",
        "auth",
        "session",
        "csrf",
        "signature",
        "private",
        "key",
        "credential",
        "access_token",
        "refresh_token",
        "bearer",
        "basic",
    ];
    patterns.iter().any(|p| lower_key.contains(p))
}

fn redact_query_string(qs: &str) -> String {
    if qs.is_empty() { return qs.to_string(); }
    let mut out_parts = Vec::new();
    for part in qs.split('&') {
        if part.is_empty() { continue; }
        let mut it = part.splitn(2, '=');
        let k = it.next().unwrap_or("");
        let v = it.next().unwrap_or("");
        let k_l = k.to_ascii_lowercase();
        if is_sensitive_key_like(&k_l) {
            out_parts.push(format!("{}=***redacted***", k));
        } else {
            out_parts.push(format!("{}={}", k, v));
        }
    }
    out_parts.join("&")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::get_max_body_size;
    
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

    #[test]
    fn test_split_set_cookie_header_simple_multiple() {
        let h = "a=1; Path=/, b=2; Path=/; Secure";
        let parts = split_set_cookie_header(h);
        assert_eq!(parts.len(), 2);
        assert!(parts[0].starts_with("a=1"));
        assert!(parts[1].starts_with("b=2"));
    }

    #[test]
    fn test_split_set_cookie_header_with_expires() {
        let h = "a=1; Expires=Tue, 31 Dec 2024 23:59:59 GMT; Path=/, b=2; Path=/";
        let parts = split_set_cookie_header(h);
        assert_eq!(parts.len(), 2);
        assert!(parts[0].contains("Expires=Tue, 31 Dec 2024 23:59:59 GMT"));
        assert!(parts[1].starts_with("b=2"));
    }

    #[test]
    fn test_write_response_multiple_set_cookie_lines() {
        // HashMapの制約によりアプリ側は1キーのみだが、
        // CGI出力側でカンマ連結を分割して複数行で出すことを確認
        let response = Response::new(200)
            .with_header(
                "Set-Cookie",
                "a=1; Path=/, b=2; Path=/; Secure, c=3; Expires=Tue, 31 Dec 2024 23:59:59 GMT; Path=/"
            )
            .with_header("Content-Type", "text/plain")
            .with_body(b"ok".to_vec());

        let mut buf: Vec<u8> = Vec::new();
        write_response_to(response, &mut buf).expect("write_response_to failed");
        let out = String::from_utf8(buf).expect("utf8");

        // ステータス行
        assert!(out.contains("Status: 200 OK"));

        // Set-Cookie が3行に分割されて出力されること
        let set_cookie_lines: Vec<&str> = out
            .lines()
            .filter(|l| l.starts_with("Set-Cookie:"))
            .collect();
        assert_eq!(set_cookie_lines.len(), 3, "expected 3 Set-Cookie lines, got: {}\n{}", set_cookie_lines.len(), out);
        assert!(out.contains("Set-Cookie: a=1; Path=/"));
        assert!(out.contains("Set-Cookie: b=2; Path=/; Secure"));
        assert!(out.contains("Set-Cookie: c=3; Expires=Tue, 31 Dec 2024 23:59:59 GMT; Path=/"));

        // Content-Length と区切り、ボディ
        assert!(out.contains("Content-Length: 2\r"));
        assert!(out.ends_with("\r\nok"));
    }

    #[test]
    fn test_redact_value_for_log() {
        // 通常の値は変更されない
        assert_eq!(redact_value_for_log("CONTENT_TYPE", "application/json"), "application/json");
        assert_eq!(redact_value_for_log("HTTP_HOST", "example.com"), "example.com");
        
        // センシティブなキーの値はマスクされる
        assert_eq!(redact_value_for_log("HTTP_AUTHORIZATION", "Bearer token123"), "***redacted***");
        assert_eq!(redact_value_for_log("HTTP_COOKIE", "session=abc123"), "***redacted***");
        assert_eq!(redact_value_for_log("HTTP_X_API_KEY", "secret-key"), "***redacted***");
        
        // QUERY_STRINGは特別な処理
        assert_eq!(redact_value_for_log("QUERY_STRING", "name=john&token=secret123"), "name=john&token=***redacted***");
        
        // 長い値はtruncateされる
        let long_value = "a".repeat(250);
        let result = redact_value_for_log("HTTP_USER_AGENT", &long_value);
        assert!(result.ends_with("...[truncated]"));
        assert_eq!(result.len(), 200 + "...[truncated]".len());
    }

    #[test]
    fn test_is_sensitive_key_like() {
        // センシティブなキー
        assert!(is_sensitive_key_like("authorization"));
        assert!(is_sensitive_key_like("http_authorization"));
        assert!(is_sensitive_key_like("cookie"));
        assert!(is_sensitive_key_like("http_cookie"));
        assert!(is_sensitive_key_like("token"));
        assert!(is_sensitive_key_like("access_token"));
        assert!(is_sensitive_key_like("secret"));
        assert!(is_sensitive_key_like("password"));
        assert!(is_sensitive_key_like("api_key"));
        assert!(is_sensitive_key_like("x-api-key"));
        assert!(is_sensitive_key_like("jwt"));
        assert!(is_sensitive_key_like("session"));
        assert!(is_sensitive_key_like("csrf"));
        assert!(is_sensitive_key_like("private"));
        
        // 大文字小文字混在（実際には関数内で小文字化される前提のため小文字で渡す）
        assert!(is_sensitive_key_like("http_authorization"));
        assert!(is_sensitive_key_like("x-api-key"));
        
        // 非センシティブなキー
        assert!(!is_sensitive_key_like("content_type"));
        assert!(!is_sensitive_key_like("host"));
        assert!(!is_sensitive_key_like("user_agent"));
        assert!(!is_sensitive_key_like("accept"));
        assert!(!is_sensitive_key_like("content_length"));
    }

    #[test]
    fn test_redact_query_string() {
        // 空文字列
        assert_eq!(redact_query_string(""), "");
        
        // センシティブなパラメータが含まれない場合
        assert_eq!(redact_query_string("name=john&age=30"), "name=john&age=30");
        
        // センシティブなパラメータがある場合
        assert_eq!(redact_query_string("name=john&token=secret123"), "name=john&token=***redacted***");
        assert_eq!(redact_query_string("api_key=secret&user=admin"), "api_key=***redacted***&user=admin");
        
        // 複数のセンシティブパラメータ
        assert_eq!(redact_query_string("token=abc&password=123&name=john"), "token=***redacted***&password=***redacted***&name=john");
        
        // URLエンコードされた値
        assert_eq!(redact_query_string("secret=encoded%20value&name=test"), "secret=***redacted***&name=test");
        
        // 値がない場合
        assert_eq!(redact_query_string("token=&name=john"), "token=***redacted***&name=john");
    }

    #[test]
    fn test_gather_cgi_panic_context() {
        use temp_env::with_vars;
        
        with_vars([
            ("QUERY_STRING", Some("name=test&token=secret")),
            ("CONTENT_TYPE", Some("application/json")),
            ("CONTENT_LENGTH", Some("123")),
            ("SERVER_NAME", Some("example.com")),
            ("HTTP_HOST", Some("example.com")),
            ("HTTP_USER_AGENT", Some("TestAgent/1.0")),
            ("HTTP_AUTHORIZATION", Some("Bearer secret-token")),
            ("HTTP_COOKIE", Some("session=abc123")),
        ], || {
            let context = gather_cgi_panic_context("POST", "/api/test");
            
            // 基本情報の確認
            assert!(context.contains("CGI panic context:"));
            assert!(context.contains("REQUEST_METHOD=POST"));
            assert!(context.contains("PATH_INFO=/api/test"));
            
            // 基本的な環境変数
            assert!(context.contains("QUERY_STRING=name=test&token=***redacted***"));
            assert!(context.contains("CONTENT_TYPE=application/json"));
            assert!(context.contains("CONTENT_LENGTH=123"));
            assert!(context.contains("SERVER_NAME=example.com"));
            
            // HTTPヘッダーセクション
            assert!(context.contains("HTTP headers:"));
            assert!(context.contains("HTTP_HOST=example.com"));
            assert!(context.contains("HTTP_USER_AGENT=TestAgent/1.0"));
            
            // センシティブ値がマスクされていることを確認
            assert!(context.contains("HTTP_AUTHORIZATION=***redacted***"));
            assert!(context.contains("HTTP_COOKIE=***redacted***"));
        });
    }

    #[test] 
    fn test_gather_cgi_panic_context_no_headers() {
        use temp_env::with_vars;
        
        // HTTPヘッダーが存在しない場合
        with_vars([
            ("CONTENT_TYPE", Some("text/plain")),
        ], || {
            let context = gather_cgi_panic_context("GET", "/");
            
            assert!(context.contains("HTTP headers:"));
            assert!(context.contains("(none)"));
        });
    }

    #[test]
    fn test_log_error_to_file() {
        use std::fs;
        use std::io::{Read, Write};
        
        let test_file = "test_runbridge_error.log";
        
        // テスト前にファイルを削除（存在する場合）
        let _ = fs::remove_file(test_file);
        
        // カスタムファイル名でログを記録する関数を作成
        fn log_error_to_test_file(message: &str, filename: &str) {
            let timestamp = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S%.3f UTC");
            let local_time = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f %Z");
            
            if let Ok(mut file) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(filename)
            {
                let _ = writeln!(file, "================================================================================");
                let _ = writeln!(file, "RUNBRIDGE CGI ERROR");
                let _ = writeln!(file, "Timestamp (UTC): {}", timestamp);
                let _ = writeln!(file, "Timestamp (Local): {}", local_time);
                let _ = writeln!(file, "Process ID: {}", std::process::id());
                let _ = writeln!(file, "--------------------------------------------------------------------------------");
                let _ = writeln!(file, "{}", message);
                let _ = writeln!(file, "================================================================================");
                let _ = writeln!(file);
            }
        }
        
        // ログメッセージを記録
        let test_message = "Test error message for unit test";
        log_error_to_test_file(test_message, test_file);
        
        // ファイルが作成されたことを確認
        assert!(std::path::Path::new(test_file).exists());
        
        // ファイル内容を読み込んで検証
        let mut content = String::new();
        if let Ok(mut file) = fs::File::open(test_file) {
            file.read_to_string(&mut content).expect("Failed to read test log file");
        }
        
        // 期待される内容が含まれていることを確認
        assert!(content.contains("RUNBRIDGE CGI ERROR"));
        assert!(content.contains("Timestamp (UTC):"));
        assert!(content.contains("Timestamp (Local):"));
        assert!(content.contains("Process ID:"));
        assert!(content.contains(test_message));
        assert!(content.contains("================================================================================"));
        
        // テスト後のクリーンアップ
        let _ = fs::remove_file(test_file);
    }
}
