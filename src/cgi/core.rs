//! CGIメイン実行ロジック

use std::env;
use log::{debug, error, info};
use tokio::task;

use crate::common::{Method, Request, Response, parse_query_string};
use crate::error::Error;
use crate::RunBridge;
use super::request::{get_cgi_headers, read_request_body};
use super::response::write_response;
use super::error_logging::{log_error_to_file, gather_cgi_panic_context};

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