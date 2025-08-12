//! エラーログとセキュリティ関連の機能

use std::env;
use std::fs::OpenOptions;
use std::io::Write;
use chrono::Local;
use log::error;

/// エラー内容をログファイルに追記する
pub fn log_error_to_file(message: &str) {
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
pub fn gather_cgi_panic_context(method: &str, path: &str) -> String {
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

pub fn redact_value_for_log(key: &str, value: &str) -> String {
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

pub fn is_sensitive_key_like(lower_key: &str) -> bool {
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

pub fn redact_query_string(qs: &str) -> String {
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