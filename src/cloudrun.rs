//! Google Cloud Run向けの実装

use std::collections::HashMap;
use std::sync::Arc;
use log::{debug, error, info};
use actix_web::{web, App, HttpRequest, HttpResponse, HttpServer, dev::Service};
use actix_web::http::header::HeaderMap;
use actix_web::web::Bytes;

use crate::common::{Method, Request, Response};
use crate::error::Error;
use crate::RunBridge;

/// actix-webのHeaderMapから共通形式のヘッダーに変換
fn convert_headers(headers: &HeaderMap) -> HashMap<String, String> {
    let mut result = HashMap::new();
    
    for (key, value) in headers.iter() {
        if let Ok(value_str) = value.to_str() {
            result.insert(key.as_str().to_string(), value_str.to_string());
        }
    }
    
    result
}

/// actix-webのリクエストから共通形式のRequestに変換
async fn convert_request(
    req: &HttpRequest,
    path: String,
    body: Option<Bytes>,
) -> Request {
    // HTTPメソッドの取得
    let method = match req.method().as_str() {
        "GET" => Method::GET,
        "POST" => Method::POST,
        "PUT" => Method::PUT,
        "DELETE" => Method::DELETE,
        "PATCH" => Method::PATCH,
        "HEAD" => Method::HEAD,
        "OPTIONS" => Method::OPTIONS,
        _ => Method::GET,
    };

    // ヘッダーの変換
    let headers = convert_headers(req.headers());

    // クエリパラメータの取得
    let query_params = req
        .query_string()
        .split('&')
        .filter(|s| !s.is_empty())
        .filter_map(|item| {
            let parts: Vec<&str> = item.splitn(2, '=').collect();
            if parts.len() == 2 {
                Some((parts[0].to_string(), parts[1].to_string()))
            } else {
                None
            }
        })
        .collect();

    // リクエストボディの処理
    let body = body.map(|b| b.to_vec());

    Request {
        method,
        path,
        query_params,
        headers,
        body,
    }
}

/// 共通形式のResponseからactix-webのHttpResponseに変換
fn convert_to_http_response(response: Response) -> HttpResponse {
    let mut builder = match response.status {
        200 => HttpResponse::Ok(),
        201 => HttpResponse::Created(),
        204 => HttpResponse::NoContent(),
        400 => HttpResponse::BadRequest(),
        401 => HttpResponse::Unauthorized(),
        403 => HttpResponse::Forbidden(),
        404 => HttpResponse::NotFound(),
        500 => HttpResponse::InternalServerError(),
        _ => HttpResponse::build(actix_web::http::StatusCode::from_u16(response.status).unwrap_or(actix_web::http::StatusCode::OK)),
    };

    // ヘッダーの設定
    for (key, value) in response.headers {
        builder.insert_header((key, value));
    }

    // ボディの設定
    if let Some(body) = response.body {
        builder.body(body)
    } else {
        builder.finish()
    }
}

/// RunBridgeアプリケーションをハンドリングするactix-web用ハンドラー
async fn handle_request(
    req: HttpRequest, 
    body: Option<Bytes>,
    app: web::Data<Arc<RunBridge>>,
) -> HttpResponse {
    let path = req.uri().path().to_string();
    let method_str = req.method().as_str();
    info!("Received request: {} {}", method_str, path);

    // リクエストの変換
    let request = convert_request(&req, path.clone(), body).await;
    
    // メソッドの変換
    let method = match Method::from_str(method_str) {
        Some(m) => m,
        None => {
            error!("Unsupported HTTP method: {}", method_str);
            return HttpResponse::MethodNotAllowed().finish();
        }
    };

    // ハンドラーの検索
    let handler = match app.find_handler(&path, &method) {
        Some(handler) => handler,
        None => {
            error!("Route not found: {} {}", method, path);
            return convert_to_http_response(Response::not_found()
                .with_body("Not Found".as_bytes().to_vec()));
        }
    };

    // ミドルウェアの適用（リクエスト前処理）
    let mut req_processed = request;
    for middleware in app.middlewares() {
        match middleware.pre_process(req_processed).await {
            Ok(processed) => req_processed = processed,
            Err(e) => {
                error!("Middleware error: {}", e);
                let status = e.status_code();
                return convert_to_http_response(Response::new(status)
                    .with_body(format!("Error: {}", e).as_bytes().to_vec()));
            }
        }
    }

    // ハンドラーの実行
    let handler_result = handler.handle(req_processed).await;

    // レスポンスの処理
    let response = match handler_result {
        Ok(res) => res,
        Err(e) => {
            error!("Handler error: {}", e);
            Response::from_error(&e)
        }
    };

    // ミドルウェアの適用（レスポンス後処理）
    let mut res_processed = response;
    for middleware in app.middlewares() {
        match middleware.post_process(res_processed).await {
            Ok(processed) => res_processed = processed,
            Err(e) => {
                error!("Middleware error in post-processing: {}", e);
                res_processed = Response::from_error(&e);
            }
        }
    }

    // レスポンスの変換と返却
    convert_to_http_response(res_processed)
}

/// アプリケーションをCloud Run/HTTPサーバーとして実行
pub async fn run_cloud_run(app: RunBridge, host: &str, port: u16) -> std::io::Result<()> {
    info!("Starting HTTP server on {}:{}", host, port);
    
    // アプリケーションをArcで包んでスレッド間で共有可能にする
    let app_data = Arc::new(app);
    
    // HTTPサーバーの構築と起動
    HttpServer::new(move || {
        let app_data = web::Data::new(app_data.clone());
        
        App::new()
            .app_data(app_data.clone())
            // すべてのリクエストをキャッチする汎用ハンドラー
            .route("/{path:.*}", web::get().to(|req, app: web::Data<Arc<RunBridge>>| 
                handle_request(req, None, app)))
            .route("/{path:.*}", web::post().to(|req, body: Option<Bytes>, app: web::Data<Arc<RunBridge>>| 
                handle_request(req, body, app)))
            .route("/{path:.*}", web::put().to(|req, body: Option<Bytes>, app: web::Data<Arc<RunBridge>>| 
                handle_request(req, body, app)))
            .route("/{path:.*}", web::delete().to(|req, app: web::Data<Arc<RunBridge>>| 
                handle_request(req, None, app)))
            .route("/{path:.*}", web::patch().to(|req, body: Option<Bytes>, app: web::Data<Arc<RunBridge>>| 
                handle_request(req, body, app)))
            .route("/{path:.*}", web::head().to(|req, app: web::Data<Arc<RunBridge>>| 
                handle_request(req, None, app)))
            .route("/{path:.*}", web::method(actix_web::http::Method::OPTIONS).to(|req, app: web::Data<Arc<RunBridge>>| 
                handle_request(req, None, app)))
    })
    .bind((host, port))?
    .run()
    .await
} 