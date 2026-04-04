//! CGIレスポンスの出力機能

use log::error;
use std::io::{self, Write};

use super::error_logging::log_error_to_file;
use super::validation::{is_valid_header_name, is_valid_header_value};
use crate::common::{Response, StatusCode};
use crate::error::Error;

pub fn write_response_to<W: Write>(mut response: Response, out: &mut W) -> Result<(), Error> {
    let mut sanitized_headers: Vec<(String, String)> = Vec::new();

    for (name, value) in &response.headers {
        if name.eq_ignore_ascii_case("Status") || name.eq_ignore_ascii_case("Content-Length") {
            continue;
        }
        if !is_valid_header_name(name) || !is_valid_header_value(value) {
            error!(
                "Invalid header detected - name: '{}', value: '{}'",
                name, value
            );
            log_error_to_file(&format!(
                "CRLF injection attempt detected in header: '{}': '{}'",
                name, value
            ));
            response = Response::new(400)
                .with_header("Content-Type", "text/plain; charset=utf-8")
                .with_body(b"Bad Request: Invalid header".to_vec());
            sanitized_headers.clear();
            break;
        }
        sanitized_headers.push((name.to_string(), value.to_string()));
    }

    let reason_phrase = StatusCode::from_u16(response.status)
        .map(|status| status.reason_phrase())
        .unwrap_or("Unknown");

    out.write_all(format!("Status: {} {}\r\n", response.status, reason_phrase).as_bytes())
        .map_err(|e| Error::InternalServerError(format!("Failed to write status line: {}", e)))?;

    for (name, value) in sanitized_headers {
        out.write_all(format!("{}: {}\r\n", name, value).as_bytes())
            .map_err(|e| Error::InternalServerError(format!("Failed to write header: {}", e)))?;
    }

    for cookie in &response.cookies {
        out.write_all(format!("Set-Cookie: {}\r\n", cookie.to_header_value()).as_bytes())
            .map_err(|e| {
                Error::InternalServerError(format!("Failed to write Set-Cookie header: {}", e))
            })?;
    }

    if let Some(body) = &response.body {
        out.write_all(format!("Content-Length: {}\r\n", body.len()).as_bytes())
            .map_err(|e| {
                Error::InternalServerError(format!("Failed to write Content-Length: {}", e))
            })?;
    }

    out.write_all(b"\r\n").map_err(|e| {
        Error::InternalServerError(format!("Failed to write header/body separator: {}", e))
    })?;

    if let Some(body) = response.body {
        out.write_all(body.as_ref()).map_err(|e| {
            Error::InternalServerError(format!("Failed to write response body: {}", e))
        })?;
    }

    Ok(())
}

pub fn write_response(response: Response) -> Result<(), Error> {
    let mut out = io::stdout().lock();
    let res = write_response_to(response, &mut out);
    out.flush()
        .map_err(|e| Error::InternalServerError(format!("Failed to flush stdout: {}", e)))?;
    res
}
