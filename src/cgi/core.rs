//! CGIメイン実行ロジック

use log::{debug, error, info};
use std::env;
use tokio::task;

use super::error_logging::{gather_cgi_panic_context, log_error_to_file};
use super::request::{get_cgi_headers, read_request_body};
use super::response::write_response;
use crate::common::{parse_cookie_header, parse_query_string, Method, Request, Response};
use crate::error::Error;
use crate::RunBridge;

pub async fn run_cgi(app: RunBridge) -> Result<(), Error> {
    let method_str = env::var("REQUEST_METHOD").map_err(|_| {
        Error::InvalidRequestBody("REQUEST_METHOD environment variable not set".to_string())
    })?;

    let method = Method::from_str(&method_str)
        .ok_or_else(|| Error::InvalidRequestBody(format!("Invalid HTTP method: {}", method_str)))?;

    let path = env::var("PATH_INFO").unwrap_or_else(|_| "/".to_string());
    let query_string = env::var("QUERY_STRING").unwrap_or_default();

    let query = parse_query_string(&query_string);
    let headers = get_cgi_headers();
    let cookies = headers
        .get("cookie")
        .map(parse_cookie_header)
        .unwrap_or_default();

    let body = match read_request_body() {
        Ok(body) => body,
        Err(Error::PayloadTooLarge(_)) => {
            let res = Response::new(413)
                .with_header("Content-Type", "text/plain")
                .with_body("Payload Too Large".as_bytes().to_vec());
            write_response(res)?;
            return Ok(());
        }
        Err(err) => return Err(err),
    };

    let mut request = Request::new(method, path.clone());
    request.query = query;
    request.headers = headers;
    request.cookies = cookies;
    request.body = body;

    if let Err(err) = request.decompress_gzip_body() {
        error!("Failed to decompress gzip body in CGI: {}", err);
        write_response(Response::from_error(&err))?;
        return Ok(());
    }

    debug!("Processing CGI request: {} {}", method, path);

    let task_result = task::spawn(async move { process_request(app, request).await }).await;

    let response = match task_result {
        Ok(response) => response,
        Err(join_err) => {
            let panic_info = if join_err.is_panic() {
                "panic occurred in handler".to_string()
            } else {
                format!("task cancelled: {}", join_err)
            };
            error!("{}", panic_info);
            log_error_to_file(&format!("{} at {} {}", panic_info, method, path));
            if join_err.is_panic() {
                let ctx = gather_cgi_panic_context(&method.to_string(), &path);
                log_error_to_file(&ctx);
            }
            Response::internal_server_error()
                .with_header("Content-Type", "text/plain")
                .with_body("Internal Server Error".as_bytes().to_vec())
        }
    };

    write_response(response)?;
    info!("CGI request processed successfully");
    Ok(())
}

async fn process_request(app: RunBridge, request: Request) -> Response {
    app.handle_request(request).await
}
