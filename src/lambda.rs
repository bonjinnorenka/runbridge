//! AWS Lambda向けの実装

use std::collections::HashMap;
use log::{debug, info, warn, error};
use lambda_runtime::{run, service_fn, Error as LambdaError, LambdaEvent};
use aws_lambda_events::event::apigw::{ApiGatewayV2httpRequest, ApiGatewayV2httpResponse};
use aws_lambda_events::http::header::{HeaderMap, HeaderName, HeaderValue};
use aws_lambda_events::encodings::Body;

use crate::common::{Method, Request, Response, get_max_body_size};
use crate::error::Error as AppError;
use crate::RunBridge;

// 共有の get_max_body_size を使用（common/utils.rs）

/// API Gateway Proxyリクエストから共通のRequestに変換
fn convert_apigw_request(event: ApiGatewayV2httpRequest) -> Result<Request, AppError> {
    // HTTPメソッドの変換
    let method = match event.request_context.http.method.as_str() {
        "GET" => Method::GET,
        "POST" => Method::POST,
        "PUT" => Method::PUT,
        "DELETE" => Method::DELETE,
        "PATCH" => Method::PATCH,
        "HEAD" => Method::HEAD,
        "OPTIONS" => Method::OPTIONS,
        _ => {
            debug!("Unknown HTTP method: {}, fallback to GET", event.request_context.http.method);
            Method::GET
        }
    };

    // パスの取得
    let path = event.request_context.http.path.unwrap_or_else(|| "/".to_string());

    // クエリパラメータの解析
    let mut query_params = HashMap::new();
    // クエリストリングパラメータはオプションではなく、デフォルト値が空のマップ
    for (key, value) in event.query_string_parameters.iter() {
        query_params.insert(key.to_string(), value.to_string());
    }

    // ヘッダーの変換
    let headers: HashMap<String, String> = event.headers.iter()
        .filter_map(|(k, v)| {
            if let Ok(v_str) = v.to_str() {
                // Request取り込み時は小文字キーに正規化
                Some((k.as_str().to_ascii_lowercase(), v_str.to_string()))
            } else {
                None
            }
        })
        .collect();

    // ボディの変換（境界検査とサイズ上限チェック）
    let body = match event.body {
        Some(body_str) => {
            let max_body_bytes = get_max_body_size();
            if event.is_base64_encoded {
                // 入力長から概算のデコード後サイズを見積り（4文字→3バイト、端数切り上げ）
                let estimated_decoded = ((body_str.len() + 3) / 4).saturating_mul(3);
                if estimated_decoded > max_body_bytes {
                    warn!(
                        "Base64 body too large: estimated {} bytes (limit {})",
                        estimated_decoded,
                        max_body_bytes
                    );
                    return Err(AppError::PayloadTooLarge(format!(
                        "Body too large (>{} bytes)",
                        max_body_bytes
                    )));
                }

                match base64::decode(&body_str) {
                    Ok(bytes) => {
                        if bytes.len() > max_body_bytes {
                            warn!(
                                "Decoded body too large: {} bytes (limit {})",
                                bytes.len(),
                                max_body_bytes
                            );
                            return Err(AppError::PayloadTooLarge(format!(
                                "Body too large (>{} bytes)",
                                max_body_bytes
                            )));
                        }
                        Some(bytes)
                    }
                    Err(e) => {
                        warn!("Base64 decode error: {}", e);
                        return Err(AppError::InvalidRequestBody(
                            "Invalid base64-encoded request body".to_string(),
                        ));
                    }
                }
            } else {
                // 非Base64ボディのサイズ検査
                if body_str.len() > max_body_bytes {
                    warn!(
                        "Body too large: {} bytes (limit {})",
                        body_str.len(),
                        max_body_bytes
                    );
                    return Err(AppError::PayloadTooLarge(format!(
                        "Body too large (>{} bytes)",
                        max_body_bytes
                    )));
                }
                Some(body_str.into_bytes())
            }
        }
        None => None,
    };

    // Requestオブジェクトの構築
    let mut request = Request::new(method, path);
    request.query_params = query_params;
    request.headers = headers;
    request.body = body;

    // パスパラメータの処理
    for (key, value) in event.path_parameters.iter() {
        request.query_params.insert(format!("path_{}", key), value.to_string());
    }

    Ok(request)
}

/// 共通のResponseからAPI Gateway Proxyレスポンスに変換
fn convert_to_apigw_response(response: Response) -> ApiGatewayV2httpResponse {
    // ボディの変換
    let (body, is_base64_encoded) = if let Some(body) = response.body {
        // テキストとして解釈できるかチェック
        match String::from_utf8(body.clone()) {
            Ok(text) => (Some(text), false),
            Err(_) => {
                // バイナリデータの場合はBase64エンコード
                (Some(base64::encode(&body)), true)
            }
        }
    } else {
        (None, false)
    };

    // ヘッダーの変換
    let mut headers = HeaderMap::new();
    for (key, value) in response.headers {
        if let (Ok(header_name), Ok(header_value)) = (
            HeaderName::try_from(key),
            HeaderValue::try_from(value)
        ) {
            headers.insert(header_name, header_value);
        }
    }

    // マルチバリューヘッダーを空のヘッダーマップで初期化
    let multi_value_headers = HeaderMap::new();

    // ボディの変換
    let body = body.map(Body::Text);

    ApiGatewayV2httpResponse {
        status_code: response.status as i64,
        headers,
        multi_value_headers,
        body,
        is_base64_encoded: is_base64_encoded,
        cookies: Vec::new(),
    }
}

/// Lambda関数のハンドラー
async fn lambda_handler(
    app: &RunBridge,
    event: LambdaEvent<ApiGatewayV2httpRequest>,
) -> Result<ApiGatewayV2httpResponse, LambdaError> {
    let (event, _context) = event.into_parts();
    
    // リクエストの変換
    let req = match convert_apigw_request(event) {
        Ok(req) => req,
        Err(e) => {
            error!("Request conversion error: {}", e);
            let error_response = Response::from_error(&e);
            return Ok(convert_to_apigw_response(error_response));
        }
    };
    info!("Received request: {} {}", req.method, req.path);

    // ハンドラーの検索
    let handler = match app.find_handler(&req.path, &req.method) {
        Some(handler) => handler,
        None => {
            error!("Route not found: {} {}", req.method, req.path);
            let error_response = Response::not_found()
                .with_body("Not Found".as_bytes().to_vec());
            return Ok(convert_to_apigw_response(error_response));
        }
    };

    // ミドルウェアの適用（リクエスト前処理）
    let mut req_processed = req;
    for middleware in app.middlewares() {
        match middleware.pre_process(req_processed).await {
            Ok(processed) => req_processed = processed,
            Err(e) => {
                error!("Middleware error: {}", e);
                let status = e.status_code();
                let error_response = Response::new(status)
                    .with_body(format!("Error: {}", e).as_bytes().to_vec());
                return Ok(convert_to_apigw_response(error_response));
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
    Ok(convert_to_apigw_response(res_processed))
}

/// アプリケーションをLambda関数として実行
pub async fn run_lambda(app: RunBridge) -> Result<(), LambdaError> {
    info!("Starting Lambda handler");
    
    let app = std::sync::Arc::new(app);

    // サービス関数の定義
    let handler_func = service_fn(move |event| {
        let app_clone = app.clone();
        async move {
            lambda_handler(&app_clone, event).await
        }
    });

    // Lambda実行ランタイムの起動
    run(handler_func).await?;
    
    Ok(())
} 
