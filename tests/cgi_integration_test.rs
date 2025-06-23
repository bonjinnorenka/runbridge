#![cfg(feature = "cgi")]

//! CGI機能の統合テスト
//!
//! CGI環境のシミュレーションを行い、実際のリクエスト処理をテストします。

use std::io::{self, Write};
use std::process::{Command, Stdio};
use temp_env::with_vars;

#[test]
fn test_cgi_hello_endpoint() {
    let output = run_cgi_with_env(
        vec![
            ("REQUEST_METHOD", Some("GET")),
            ("PATH_INFO", Some("/")),
            ("QUERY_STRING", Some("")),
        ],
        "".as_bytes(),
    );

    // 出力を確認
    let stdout = String::from_utf8_lossy(&output.stdout);
    
    // ステータスコードが200であることを確認
    assert!(stdout.contains("Status: 200 OK"));
    
    // Content-Typeヘッダーを確認
    assert!(stdout.contains("Content-Type: application/json"));
    
    // レスポンスボディを確認
    assert!(stdout.contains("Hello from RunBridge CGI"));
}

#[test]
fn test_cgi_echo_endpoint_get() {
    let output = run_cgi_with_env(
        vec![
            ("REQUEST_METHOD", Some("GET")),
            ("PATH_INFO", Some("/echo")),
            ("QUERY_STRING", Some("name=test&value=123")),
            ("HTTP_CONTENT_TYPE", Some("application/json")),
            ("HTTP_X_CUSTOM_HEADER", Some("TestValue")),
        ],
        "".as_bytes(),
    );

    // 出力を確認
    let stdout = String::from_utf8_lossy(&output.stdout);
    
    // ステータスコードが200であることを確認
    assert!(stdout.contains("Status: 200 OK"));
    
    // Content-Typeヘッダーを確認
    assert!(stdout.contains("Content-Type: application/json"));
    
    // レスポンスボディを確認
    assert!(stdout.contains("\"method\":\"GET\""));
    assert!(stdout.contains("\"path\":\"/echo\""));
    assert!(stdout.contains("\"name\":\"test\""));
    assert!(stdout.contains("\"value\":\"123\""));
    assert!(stdout.contains("\"Content-Type\":\"application/json\""));
    assert!(stdout.contains("\"X-Custom-Header\":\"TestValue\""));
}

#[test]
fn test_cgi_echo_endpoint_post() {
    let json_body = r#"{"message":"Hello, world!"}"#;
    
    let output = run_cgi_with_env(
        vec![
            ("REQUEST_METHOD", Some("POST")),
            ("PATH_INFO", Some("/echo")),
            ("QUERY_STRING", Some("")),
            ("CONTENT_TYPE", Some("application/json")),
            ("CONTENT_LENGTH", Some(&json_body.len().to_string())),
        ],
        json_body.as_bytes(),
    );

    // 出力を確認
    let stdout = String::from_utf8_lossy(&output.stdout);
    
    // ステータスコードが200であることを確認
    assert!(stdout.contains("Status: 200 OK"));
    
    // Content-Typeヘッダーを確認
    assert!(stdout.contains("Content-Type: application/json"));
    
    // レスポンスボディを確認
    assert!(stdout.contains("\"method\":\"POST\""));
    assert!(stdout.contains("\"path\":\"/echo\""));
    assert!(stdout.contains("\"body\""));
    assert!(stdout.contains("\"message\":\"Hello, world!\""));
}

#[test]
fn test_cgi_not_found() {
    let output = run_cgi_with_env(
        vec![
            ("REQUEST_METHOD", Some("GET")),
            ("PATH_INFO", Some("/not-exists")),
            ("QUERY_STRING", Some("")),
        ],
        "".as_bytes(),
    );

    // 出力を確認
    let stdout = String::from_utf8_lossy(&output.stdout);
    
    // ステータスコードが404であることを確認
    assert!(stdout.contains("Status: 404 Not Found"));
}

#[test]
fn test_cgi_panic_handling() {
    let output = run_cgi_with_env(
        vec![
            ("REQUEST_METHOD", Some("GET")),
            ("PATH_INFO", Some("/panic")),
            ("QUERY_STRING", Some("")),
        ],
        "".as_bytes(),
    );

    // 出力を確認
    let stdout = String::from_utf8_lossy(&output.stdout);
    
    // パニックが発生しても500エラーが返されることを確認
    assert!(stdout.contains("Status: 500 Internal Server Error"));
    assert!(stdout.contains("Content-Type: text/plain"));
    assert!(stdout.contains("Internal Server Error"));
}

/// CGI環境をシミュレートして実行
fn run_cgi_with_env(env_vars: Vec<(&str, Option<&str>)>, stdin_data: &[u8]) -> std::process::Output {
    // 現在のパスでcargo buildを実行してバイナリをビルド
    let build_status = Command::new("cargo")
        .args(["build", "--features", "cgi", "--bin", "runbridge-cgi"])
        .status()
        .expect("Failed to build CGI binary");
        
    assert!(build_status.success(), "Build failed");
    
    // 実行ファイルへのパスを取得
    let cgi_binary_path = "target/debug/runbridge-cgi";
    
    // with_vars内でCommandを実行
    with_vars(env_vars, || {
        let mut child = Command::new(cgi_binary_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("Failed to spawn CGI process");
            
        // 標準入力にデータを書き込む
        if !stdin_data.is_empty() {
            let mut stdin = child.stdin.take().expect("Failed to open stdin");
            stdin.write_all(stdin_data).expect("Failed to write to stdin");
            // ここでstdinはドロップされ、パイプが閉じられる
        }
        
        // プロセスの実行が完了するまで待機し、Output構造体を取得
        child.wait_with_output().expect("Failed to wait for CGI process")
    })
} 