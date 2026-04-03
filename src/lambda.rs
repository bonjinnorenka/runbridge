//! AWS Lambda向けの実装

use aws_lambda_events::encodings::Body;
use aws_lambda_events::event::apigw::{ApiGatewayV2httpRequest, ApiGatewayV2httpResponse};
use aws_lambda_events::http::header::{HeaderMap as AwsHeaderMap, HeaderName, HeaderValue};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use lambda_runtime::{run, service_fn, Error as LambdaError, LambdaEvent};
use log::{error, info, warn};
use std::collections::HashMap;
use std::str::FromStr;

use crate::common::{
    get_max_body_size, parse_cookie_header, parse_cookie_strings, parse_query_string, HeaderMap,
    Method, Request, Response,
};
use crate::error::Error as AppError;
use crate::RunBridge;

fn parse_http_method(method: &str) -> Result<Method, AppError> {
    Method::from_str(method).map_err(|_| {
        warn!("Unsupported HTTP method in Lambda request: {}", method);
        AppError::InvalidRequestBody(format!("Unsupported HTTP method: {}", method))
    })
}

fn convert_apigw_request(event: ApiGatewayV2httpRequest) -> Result<Request, AppError> {
    let method = parse_http_method(event.request_context.http.method.as_str())?;
    let path = event
        .raw_path
        .or(event.request_context.http.path)
        .unwrap_or_else(|| "/".to_string());

    let query = if let Some(raw_query) = event.raw_query_string.as_deref() {
        parse_query_string(raw_query)
    } else {
        let mut query = crate::common::QueryMap::new();
        for (key, value) in event.query_string_parameters.iter() {
            query.append(key.to_string(), value.to_string());
        }
        query
    };

    let mut headers = HeaderMap::new();
    for (key, value) in event.headers.iter() {
        if let Ok(value_str) = value.to_str() {
            headers.append(key.as_str().to_string(), value_str.to_string());
        }
    }

    let cookies = if let Some(cookies) = event.cookies {
        parse_cookie_strings(cookies)
    } else if let Some(cookie_header) = headers.get("cookie") {
        parse_cookie_header(cookie_header)
    } else {
        Vec::new()
    };

    let body = match event.body {
        Some(body_str) => {
            let max_body_bytes = get_max_body_size();
            if event.is_base64_encoded {
                let estimated_decoded = body_str.len().div_ceil(4).saturating_mul(3);
                if estimated_decoded > max_body_bytes {
                    return Err(AppError::PayloadTooLarge(format!(
                        "Body too large (>{} bytes)",
                        max_body_bytes
                    )));
                }

                let bytes = STANDARD.decode(body_str.as_bytes()).map_err(|_| {
                    AppError::InvalidRequestBody("Invalid base64-encoded request body".to_string())
                })?;
                if bytes.len() > max_body_bytes {
                    return Err(AppError::PayloadTooLarge(format!(
                        "Body too large (>{} bytes)",
                        max_body_bytes
                    )));
                }
                Some(bytes.into())
            } else {
                if body_str.len() > max_body_bytes {
                    return Err(AppError::PayloadTooLarge(format!(
                        "Body too large (>{} bytes)",
                        max_body_bytes
                    )));
                }
                Some(body_str.into_bytes().into())
            }
        }
        None => None,
    };

    let mut request = Request::new(method, path);
    request.query = query;
    request.headers = headers;
    request.path_params = event.path_parameters;
    request.cookies = cookies;
    request.body = body;

    request.decompress_gzip_body()?;

    Ok(request)
}

fn convert_to_apigw_response(response: Response) -> ApiGatewayV2httpResponse {
    let (body, is_base64_encoded) = if let Some(body) = response.body {
        match String::from_utf8(body.to_vec()) {
            Ok(text) => (Some(text), false),
            Err(_) => (Some(STANDARD.encode(body)), true),
        }
    } else {
        (None, false)
    };

    let mut grouped: HashMap<String, (String, Vec<String>)> = HashMap::new();
    for (key, value) in &response.headers {
        let lowered = key.to_ascii_lowercase();
        grouped
            .entry(lowered)
            .and_modify(|(_, values)| values.push(value.to_string()))
            .or_insert_with(|| (key.to_string(), vec![value.to_string()]));
    }

    let mut headers = AwsHeaderMap::new();
    let mut multi_value_headers = AwsHeaderMap::new();

    for (_, (original_name, values)) in grouped {
        if values.len() == 1 {
            if let (Ok(header_name), Ok(header_value)) = (
                HeaderName::try_from(original_name.as_str()),
                HeaderValue::try_from(values[0].as_str()),
            ) {
                headers.insert(header_name, header_value);
            }
        } else if let Ok(header_name) = HeaderName::try_from(original_name.as_str()) {
            for value in values {
                if let Ok(header_value) = HeaderValue::try_from(value.as_str()) {
                    multi_value_headers.append(header_name.clone(), header_value);
                }
            }
        }
    }

    ApiGatewayV2httpResponse {
        status_code: response.status as i64,
        headers,
        multi_value_headers,
        body: body.map(Body::Text),
        is_base64_encoded,
        cookies: response
            .cookies
            .into_iter()
            .map(|cookie| cookie.to_header_value())
            .collect(),
    }
}

async fn lambda_handler(
    app: &RunBridge,
    event: LambdaEvent<ApiGatewayV2httpRequest>,
) -> Result<ApiGatewayV2httpResponse, LambdaError> {
    let (event, _) = event.into_parts();

    let req = match convert_apigw_request(event) {
        Ok(req) => req,
        Err(err) => {
            error!("Request conversion error: {}", err);
            return Ok(convert_to_apigw_response(Response::from_error(&err)));
        }
    };

    info!("Received request: {} {}", req.method, req.path);
    let response = app.handle_request(req).await;
    Ok(convert_to_apigw_response(response))
}

pub async fn run_lambda(app: RunBridge) -> Result<(), LambdaError> {
    info!("Starting Lambda handler");

    let app = std::sync::Arc::new(app);
    let handler_func = service_fn(move |event| {
        let app_clone = app.clone();
        async move { lambda_handler(&app_clone, event).await }
    });

    run(handler_func).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{convert_apigw_request, convert_to_apigw_response, parse_http_method};
    use crate::common::Method;
    use crate::common::{Cookie, Response};
    use crate::error::Error as AppError;
    use aws_lambda_events::event::apigw::ApiGatewayV2httpRequest;
    use aws_lambda_events::http::header::{HeaderName, HeaderValue};

    #[test]
    fn parse_http_method_accepts_supported_methods_case_insensitive() {
        assert_eq!(parse_http_method("GET").unwrap(), Method::GET);
        assert_eq!(parse_http_method("post").unwrap(), Method::POST);
        assert_eq!(parse_http_method("Options").unwrap(), Method::OPTIONS);
    }

    #[test]
    fn parse_http_method_rejects_unknown_method() {
        let err = parse_http_method("TRACE").expect_err("TRACE must be rejected");
        match err {
            AppError::InvalidRequestBody(message) => {
                assert!(message.contains("Unsupported HTTP method: TRACE"));
            }
            other => panic!("unexpected error variant: {:?}", other),
        }
    }

    #[test]
    fn convert_apigw_request_extracts_query_and_cookies() {
        let mut event = ApiGatewayV2httpRequest::default();
        event.raw_path = Some("/items/123".to_string());
        event.raw_query_string = Some("tag=a&tag=b".to_string());
        event.cookies = Some(vec!["session=abc".to_string(), "theme=dark".to_string()]);
        event.request_context.http.method = aws_lambda_events::http::Method::GET;
        event.request_context.http.path = Some("/items/123".to_string());
        event.headers.insert(
            HeaderName::from_static("x-test"),
            HeaderValue::from_static("one"),
        );

        let request = convert_apigw_request(event).unwrap();
        assert_eq!(request.query.get_all("tag"), vec!["a", "b"]);
        assert_eq!(request.headers.get("x-test"), Some("one"));
        assert_eq!(request.cookies.len(), 2);
        assert_eq!(request.cookies[0].name, "session");
    }

    #[test]
    fn convert_to_apigw_response_preserves_multiple_cookies() {
        let response = Response::ok()
            .append_header("X-Test", "one")
            .append_header("X-Test", "two")
            .with_cookie(Cookie::new("session", "abc"))
            .with_cookie(Cookie::new("theme", "dark"));

        let converted = convert_to_apigw_response(response);
        assert_eq!(converted.cookies.len(), 2);
        assert!(converted
            .cookies
            .iter()
            .any(|cookie| cookie.starts_with("session=abc")));
        assert!(converted
            .cookies
            .iter()
            .any(|cookie| cookie.starts_with("theme=dark")));
    }
}
