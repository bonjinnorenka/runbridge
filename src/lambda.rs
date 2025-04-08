//! AWS Lambda向けの実装

use std::collections::HashMap;
use log::{debug, error, info};
use lambda_runtime::{run, service_fn, Error as LambdaError, LambdaEvent};
use aws_lambda_events::event::apigw::{ApiGatewayV2httpRequest, ApiGatewayV2httpResponse};
use aws_lambda_events::http::header::{HeaderMap, HeaderName, HeaderValue};
use aws_lambda_events::encodings::Body;

use crate::common::{Method, Request, Response};
use crate::RunBridge;

/// API Gateway Proxyリクエストから共通のRequestに変換
fn convert_apigw_request(event: ApiGatewayV2httpRequest) -> Request {
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
                Some((k.to_string(), v_str.to_string()))
            } else {
                None
            }
        })
        .collect();

    // ボディの変換
    let body = event.body.map(|body| {
        if event.is_base64_encoded {
            base64::decode(body).unwrap_or_default()
        } else {
            body.into_bytes()
        }
    });

    // Requestオブジェクトの構築
    let mut request = Request {
        method,
        path,
        query_params,
        headers,
        body,
    };

    // パスパラメータの処理
    for (key, value) in event.path_parameters.iter() {
        request.query_params.insert(format!("path_{}", key), value.to_string());
    }

    request
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
    let req = convert_apigw_request(event);
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
            let status = e.status_code();
            Response::new(status)
                .with_body(format!("Error: {}", e).as_bytes().to_vec())
        }
    };

    // ミドルウェアの適用（レスポンス後処理）
    let mut res_processed = response;
    for middleware in app.middlewares() {
        match middleware.post_process(res_processed).await {
            Ok(processed) => res_processed = processed,
            Err(e) => {
                error!("Middleware error in post-processing: {}", e);
                let status = e.status_code();
                res_processed = Response::new(status)
                    .with_body(format!("Error: {}", e).as_bytes().to_vec());
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