//! CGIモジュールのテスト

use std::io::Write;

use crate::common::{parse_query_string, get_max_body_size, Response};
use super::request::get_cgi_headers;
use super::validation::{is_valid_header_name, is_valid_header_value};
use super::response::{write_response_to, split_set_cookie_header};
use super::error_logging::{redact_value_for_log, is_sensitive_key_like, redact_query_string, gather_cgi_panic_context};

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
    use std::io::Read;
    
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