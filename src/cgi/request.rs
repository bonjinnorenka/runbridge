//! CGIリクエストの処理機能

use std::env;
use std::io::{self, Read};

use bytes::Bytes;

use super::validation::{is_valid_header_name, is_valid_header_value};
use crate::common::{get_max_body_size, HeaderMap};
use crate::error::Error;

/// 環境変数からHTTPヘッダーを取得する
pub fn get_cgi_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    for (key, value) in env::vars() {
        let header_name = if key.starts_with("HTTP_") {
            let header_parts: Vec<&str> = key[5..].split('_').collect();
            header_parts
                .iter()
                .map(|part| {
                    let mut chars = part.chars();
                    match chars.next() {
                        None => String::new(),
                        Some(c) => {
                            c.to_ascii_uppercase().to_string()
                                + &chars.as_str().to_ascii_lowercase()
                        }
                    }
                })
                .collect::<Vec<String>>()
                .join("-")
        } else if key == "CONTENT_TYPE" || key == "CONTENT_LENGTH" {
            let header_parts: Vec<&str> = key.split('_').collect();
            header_parts
                .iter()
                .map(|part| {
                    let mut chars = part.chars();
                    match chars.next() {
                        None => String::new(),
                        Some(c) => {
                            c.to_ascii_uppercase().to_string()
                                + &chars.as_str().to_ascii_lowercase()
                        }
                    }
                })
                .collect::<Vec<String>>()
                .join("-")
        } else {
            continue;
        };

        if !is_valid_header_name(&header_name) || !is_valid_header_value(&value) {
            continue;
        }

        headers.append(header_name, value);
    }
    headers
}

/// リクエストボディを標準入力から読み込む
pub fn read_request_body() -> Result<Option<Bytes>, Error> {
    if let Ok(content_length_str) = env::var("CONTENT_LENGTH") {
        if let Ok(content_length) = content_length_str.parse::<usize>() {
            if content_length > 0 {
                let max_body_size = get_max_body_size();
                if content_length > max_body_size {
                    return Err(Error::PayloadTooLarge(format!(
                        "Request body size {} bytes exceeds maximum allowed size {} bytes",
                        content_length, max_body_size
                    )));
                }

                let mut buffer = vec![0u8; content_length];
                io::stdin().read_exact(&mut buffer).map_err(|e| {
                    Error::InvalidRequestBody(format!("Failed to read request body: {}", e))
                })?;
                return Ok(Some(Bytes::from(buffer)));
            }
        }
    }

    Ok(None)
}
