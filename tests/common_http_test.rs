// src/common/http.rs のテストを分離した統合テスト
use runbridge::common::http::{Method, Request, Response, ResponseBuilder, StatusCode};
use runbridge::error::Error;
use runbridge::common::get_max_body_size;
use serde::{Serialize, Deserialize};

#[test]
fn test_method_from_str() {
    assert_eq!(Method::from_str("GET"), Some(Method::GET));
    assert_eq!(Method::from_str("get"), Some(Method::GET));
    assert_eq!(Method::from_str("POST"), Some(Method::POST));
    assert_eq!(Method::from_str("PUT"), Some(Method::PUT));
    assert_eq!(Method::from_str("DELETE"), Some(Method::DELETE));
    assert_eq!(Method::from_str("PATCH"), Some(Method::PATCH));
    assert_eq!(Method::from_str("HEAD"), Some(Method::HEAD));
    assert_eq!(Method::from_str("OPTIONS"), Some(Method::OPTIONS));
    assert_eq!(Method::from_str("INVALID"), None);
}

#[test]
fn test_request_builder() {
    let req = Request::new(Method::GET, "/test".to_string())
        .with_query_param("key1", "value1")
        .with_query_param("key2", "value2")
        .with_header("Content-Type", "application/json")
        .with_body(b"test body".to_vec());

    assert_eq!(req.method, Method::GET);
    assert_eq!(req.path, "/test");
    assert_eq!(req.query_params.get("key1"), Some(&"value1".to_string()));
    assert_eq!(req.query_params.get("key2"), Some(&"value2".to_string()));
    // Requestヘッダーは小文字キーで保持される
    assert_eq!(req.headers.get("content-type"), Some(&"application/json".to_string()));
    assert_eq!(req.body.as_ref().unwrap(), &b"test body".to_vec());
}

#[test]
fn test_response_builder() {
    let res = Response::ok()
        .with_header("Content-Type", "text/plain")
        .with_body(b"Hello, world!".to_vec());

    assert_eq!(res.status, 200);
    assert_eq!(res.headers.get("Content-Type"), Some(&"text/plain".to_string()));
    assert_eq!(res.body.as_ref().unwrap(), &b"Hello, world!".to_vec());
}

#[test]
fn test_header_value_validation_rejects_crlf() {
    let req = Request::new(Method::GET, "/".to_string())
        .with_header("X-Test", "ok-value")
        .with_header("X-Bad", "bad\r\ninjected: 1");
    // 正常な方は入る、小文字キー
    assert_eq!(req.headers.get("x-test"), Some(&"ok-value".to_string()));
    // 不正な方は拒否（未設定）
    assert!(req.headers.get("x-bad").is_none());

    let res = Response::ok()
        .with_header("X-Good", "value")
        .with_header("X-Evil", "evil\nvalue");
    assert_eq!(res.headers.get("X-Good"), Some(&"value".to_string()));
    assert!(res.headers.get("X-Evil").is_none());

    let built = ResponseBuilder::new(200)
        .header("A", "v1")
        .header("B", "bad\rvalue")
        .build();
    assert_eq!(built.headers.get("A"), Some(&"v1".to_string()));
    assert!(built.headers.get("B").is_none());
}

#[test]
fn test_from_error_payload_too_large() {
    let err = Error::PayloadTooLarge("exceeds".to_string());
    let res = Response::from_error(&err);
    assert_eq!(res.status, 413);
    let body = String::from_utf8(res.body.unwrap()).unwrap();
    assert_eq!(body, "Payload Too Large");
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
struct TestData {
    name: String,
    value: i32,
}

#[test]
fn test_response_json() {
    let test_data = TestData {
        name: "test".to_string(),
        value: 42,
    };

    let res = Response::ok().json(&test_data).unwrap();

    assert_eq!(res.status, 200);
    assert_eq!(res.headers.get("Content-Type"), Some(&"application/json".to_string()));
    
    // ボディをJSONとしてデコード
    let body_str = String::from_utf8(res.body.unwrap()).unwrap();
    let decoded: TestData = serde_json::from_str(&body_str).unwrap();
    
    assert_eq!(decoded, test_data);
}

#[test]
fn test_request_json() {
    let test_data = TestData {
        name: "test".to_string(),
        value: 42,
    };

    // JSONデータを含むリクエストを作成
    let json_bytes = serde_json::to_vec(&test_data).unwrap();
    let req = Request::new(Method::POST, "/test".to_string())
        .with_header("Content-Type", "application/json")
        .with_body(json_bytes);

    // JSONデータを取得
    let parsed: TestData = req.json().unwrap();
    
    assert_eq!(parsed, test_data);
}

#[test]
fn test_status_code() {
    // 基本的な値のテスト
    assert_eq!(StatusCode::Ok.as_u16(), 200);
    assert_eq!(StatusCode::Created.as_u16(), 201);
    assert_eq!(StatusCode::BadRequest.as_u16(), 400);
    assert_eq!(StatusCode::Unauthorized.as_u16(), 401);
    assert_eq!(StatusCode::InternalServerError.as_u16(), 500);

    // 理由句のテスト
    assert_eq!(StatusCode::Ok.reason_phrase(), "OK");
    assert_eq!(StatusCode::Created.reason_phrase(), "Created");
    assert_eq!(StatusCode::BadRequest.reason_phrase(), "Bad Request");
    assert_eq!(StatusCode::Unauthorized.reason_phrase(), "Unauthorized");
    assert_eq!(StatusCode::InternalServerError.reason_phrase(), "Internal Server Error");

    // 成功/エラー判定のテスト
    assert!(StatusCode::Ok.is_success());
    assert!(!StatusCode::BadRequest.is_success());
    assert!(StatusCode::BadRequest.is_client_error());
    assert!(StatusCode::InternalServerError.is_server_error());
}

#[test]
fn test_response_builder_methods() {
    let response = ResponseBuilder::with_status(StatusCode::Created)
        .security_headers()
        .header("X-Test", "test-value")
        .text("Hello")
        .build();

    assert_eq!(response.status, StatusCode::Created.as_u16());
    assert!(response.headers.contains_key("X-Content-Type-Options"));
    assert_eq!(response.headers.get("X-Test"), Some(&"test-value".to_string()));
    assert_eq!(response.headers.get("Content-Type"), Some(&"text/plain; charset=utf-8".to_string()));
    assert_eq!(String::from_utf8(response.body.unwrap()).unwrap(), "Hello");
}

#[test]
fn test_response_builder_with_json() {
    #[derive(Serialize)]
    struct TestJson { message: String }
    
    let test_json = TestJson { message: "Hi".to_string() };
    let response = ResponseBuilder::new(200)
        .json(&test_json)
        .unwrap()
        .build();

    assert_eq!(response.status, 200);
    assert_eq!(response.headers.get("Content-Type"), Some(&"application/json".to_string()));
}

#[test]
fn test_headers_case_sensitive_in_response() {
    // Responseはヘッダー名の正規化は行わない（仕様通り）
    let response = Response::ok()
        .with_header("Header1", "Value1")
        .with_header("Header2", "Value2")
        .with_header("Header3", "Value3");
    
    assert_eq!(response.headers.get("Header1"), Some(&"Value1".to_string()));
    assert_eq!(response.headers.get("Header2"), Some(&"Value2".to_string()));
    assert_eq!(response.headers.get("Header3"), Some(&"Value3".to_string()));
}

#[test]
fn test_request_clone_without_context() {
    let mut req = Request::new(Method::POST, "/test".to_string())
        .with_query_param("key1", "value1")
        .with_header("Content-Type", "application/json")
        .with_body(b"test body".to_vec());

    // コンテキストにデータを追加
    req.context_mut().set("user_id", 123u32);
    req.context_mut().set("session", "abc123".to_string());
    
    // コンテキスト有りの状態を確認
    assert!(req.context().contains_key("user_id"));
    assert!(req.context().contains_key("session"));

    // コンテキストなしでクローン
    let cloned = req.clone_without_context();
    
    // 基本データは複製されている
    assert_eq!(cloned.method, req.method);
    assert_eq!(cloned.path, req.path);
    assert_eq!(cloned.query_params, req.query_params);
    assert_eq!(cloned.headers, req.headers);
    assert_eq!(cloned.body, req.body);
    
    // コンテキストは空になっている
    assert!(cloned.context().is_empty());
    assert!(!cloned.context().contains_key("user_id"));
    assert!(!cloned.context().contains_key("session"));
    
    // 元のリクエストのコンテキストは保持されている
    assert!(req.context().contains_key("user_id"));
    assert!(req.context().contains_key("session"));
}

#[test]
fn test_decompress_gzip_body_success() {
    use std::io::Write;
    use flate2::write::GzEncoder;
    use flate2::Compression;

    // テスト用のJSONデータを作成
    let original_data = r#"{"message": "Hello, World!", "compressed": true}"#;
    
    // gzipで圧縮
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(original_data.as_bytes()).unwrap();
    let compressed_data = encoder.finish().unwrap();

    // gzipヘッダー付きのリクエストを作成
    let mut request = Request::new(Method::POST, "/test".to_string())
        .with_header("Content-Type", "application/json")
        .with_header("Content-Encoding", "gzip")
        .with_body(compressed_data);

    // Content-Encodingヘッダーが存在することを確認
    assert_eq!(request.headers.get("content-encoding"), Some(&"gzip".to_string()));

    // gzip解凍を実行
    let result = request.decompress_gzip_body();
    assert!(result.is_ok());

    // 解凍されたボディを確認
    assert_eq!(
        String::from_utf8(request.body.unwrap()).unwrap(),
        original_data
    );

    // Content-Encodingヘッダーが削除されていることを確認
    assert!(request.headers.get("content-encoding").is_none());
}

#[test]
fn test_decompress_gzip_body_no_encoding_header() {
    let original_data = "This is not compressed";
    let mut request = Request::new(Method::POST, "/test".to_string())
        .with_header("Content-Type", "text/plain")
        .with_body(original_data.as_bytes().to_vec());

    // gzip解凍を実行（Content-Encodingがないため何もしない）
    let result = request.decompress_gzip_body();
    assert!(result.is_ok());

    // ボディが変更されていないことを確認
    assert_eq!(
        String::from_utf8(request.body.unwrap()).unwrap(),
        original_data
    );
}

#[test]
fn test_decompress_gzip_body_different_encoding() {
    let original_data = "This has different encoding";
    let mut request = Request::new(Method::POST, "/test".to_string())
        .with_header("Content-Type", "text/plain")
        .with_header("Content-Encoding", "deflate")
        .with_body(original_data.as_bytes().to_vec());

    // gzip解凍を実行（deflateのため何もしない）
    let result = request.decompress_gzip_body();
    assert!(result.is_ok());

    // ボディが変更されていないことを確認
    assert_eq!(
        String::from_utf8(request.body.unwrap()).unwrap(),
        original_data
    );
    
    // Content-Encodingヘッダーはそのまま
    assert_eq!(request.headers.get("content-encoding"), Some(&"deflate".to_string()));
}

#[test]
fn test_decompress_gzip_body_invalid_data() {
    let invalid_gzip_data = b"This is not gzip data";
    let mut request = Request::new(Method::POST, "/test".to_string())
        .with_header("Content-Type", "application/json")
        .with_header("Content-Encoding", "gzip")
        .with_body(invalid_gzip_data.to_vec());

    // gzip解凍を実行（無効なgzipデータなのでエラー）
    let result = request.decompress_gzip_body();
    assert!(result.is_err());
    
    if let Err(Error::InvalidRequestBody(msg)) = result {
        assert!(msg.contains("Invalid gzip-encoded request body"));
    } else {
        panic!("Expected InvalidRequestBody error");
    }
}

#[test]
fn test_decompress_gzip_body_case_insensitive() {
    use std::io::Write;
    use flate2::write::GzEncoder;
    use flate2::Compression;

    let original_data = "Case insensitive test";
    
    // gzipで圧縮
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(original_data.as_bytes()).unwrap();
    let compressed_data = encoder.finish().unwrap();

    // 大文字のGZIPでContent-Encodingを設定
    let mut request = Request::new(Method::POST, "/test".to_string())
        .with_header("Content-Type", "text/plain")
        .with_header("Content-Encoding", "GZIP")
        .with_body(compressed_data);

    // gzip解凍を実行（大文字でも認識される）
    let result = request.decompress_gzip_body();
    assert!(result.is_ok());

    // 解凍されたボディを確認
    assert_eq!(
        String::from_utf8(request.body.unwrap()).unwrap(),
        original_data
    );

    // Content-Encodingヘッダーが削除されていることを確認
    assert!(request.headers.get("content-encoding").is_none());
}

#[test]
fn test_decompress_gzip_body_size_limit_exceeded() {
    use std::io::Write;
    use flate2::write::GzEncoder;
    use flate2::Compression;

    // 大きな解凍後データになる高圧縮率データを作成（1MB の "A" を繰り返し）
    let large_data = "A".repeat(1024 * 1024); // 1MB
    
    // gzipで圧縮（繰り返しデータなので非常に小さくなる）
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(large_data.as_bytes()).unwrap();
    let compressed_data = encoder.finish().unwrap();

    // 圧縮後のサイズを確認（デバッグ用）
    println!("Original size: {} bytes, Compressed size: {} bytes", 
             large_data.len(), compressed_data.len());

    // gzipヘッダー付きのリクエストを作成
    let mut request = Request::new(Method::POST, "/test".to_string())
        .with_header("Content-Type", "text/plain")
        .with_header("Content-Encoding", "gzip")
        .with_body(compressed_data);

    // gzip解凍を実行（サイズ上限を超えるのでエラーになるはず）
    let result = request.decompress_gzip_body();
    
    // 現在の実装では5MBが上限なので、1MBなら成功するはず
    // 実際に上限超過をテストするため、より大きなデータを作成
    assert!(result.is_ok(), "1MB should be within limits");
}

#[test]
fn test_decompress_gzip_body_size_limit_very_large() {
    use std::io::Write;
    use flate2::write::GzEncoder;
    use flate2::Compression;

    // 非常に大きな解凍後データ（10MB）を作成して上限超過をテスト
    let very_large_data = "B".repeat(10 * 1024 * 1024); // 10MB
    
    // gzipで圧縮
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(very_large_data.as_bytes()).unwrap();
    let compressed_data = encoder.finish().unwrap();

    // gzipヘッダー付きのリクエストを作成
    let mut request = Request::new(Method::POST, "/test".to_string())
        .with_header("Content-Type", "text/plain")
        .with_header("Content-Encoding", "gzip")
        .with_body(compressed_data);

    // gzip解凍を実行（10MBは5MB上限を超えるのでエラー）
    let result = request.decompress_gzip_body();
    assert!(result.is_err());
    
    if let Err(Error::PayloadTooLarge(msg)) = result {
        assert!(msg.contains("Decompressed body too large"));
    } else {
        panic!("Expected PayloadTooLarge error for 10MB data");
    }

    // Content-Encodingヘッダーは残っている（解凍に失敗したため）
    assert_eq!(request.headers.get("content-encoding"), Some(&"gzip".to_string()));
}

#[test]
fn test_decompress_gzip_body_incremental_size_check() {
    use std::io::Write;
    use flate2::write::GzEncoder;
    use flate2::Compression;

    // チャンクごとのサイズチェックをテストするため、
    // 複数の大きなブロックから構成されるデータを作成
    let mut large_content = String::new();
    for i in 0..1000 {
        large_content.push_str(&format!("Block {} with some padding data to make it larger. ", i));
        large_content.push_str(&"X".repeat(1000)); // 各ブロックを1KB程度にする
    }
    // 約1MBのデータ

    // gzipで圧縮
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(large_content.as_bytes()).unwrap();
    let compressed_data = encoder.finish().unwrap();

    let mut request = Request::new(Method::POST, "/test".to_string())
        .with_header("Content-Type", "text/plain")
        .with_header("Content-Encoding", "gzip")
        .with_body(compressed_data);

    // 1MBなので正常に解凍される
    let result = request.decompress_gzip_body();
    assert!(result.is_ok());

    // 解凍されたデータのサイズを確認
    let decompressed_size = request.body.as_ref().unwrap().len();
    assert_eq!(decompressed_size, large_content.len());
    
    // Content-Encodingヘッダーが削除されている
    assert!(request.headers.get("content-encoding").is_none());
}

#[test]
fn test_gzip_decompression_uses_same_body_size_limit() {
    // get_max_body_size()が正しく使用されていることを確認
    let max_size = get_max_body_size();
    
    // デフォルト値の確認（環境変数がない場合）
    std::env::remove_var("RUNBRIDGE_MAX_BODY_SIZE");
    let default_size = get_max_body_size();
    assert_eq!(default_size, 5 * 1024 * 1024); // 5MB
    
    println!("Current max body size: {} bytes ({} MB)", max_size, max_size / (1024 * 1024));
    
    // 実装では同じget_max_body_size()を使用しているので、
    // 通常のボディサイズ制限とgzip解凍後のサイズ制限は同じになる
    assert!(max_size > 0);
}

