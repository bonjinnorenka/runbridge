//! CGIレスポンスの出力機能

use std::io::{self, Write};
use log::error;

use crate::common::Response;
use crate::error::Error;
use super::validation::{is_valid_header_name, is_valid_header_value};
use super::error_logging::log_error_to_file;

/// レスポンスを任意のライターへ書き出す（テスト容易化のため公開しない）
pub fn write_response_to<W: Write>(mut response: Response, out: &mut W) -> Result<(), Error> {
    // 出力前に全ヘッダーを検証し、予約ヘッダーを除外する
    let mut sanitized_headers: Vec<(String, String)> = Vec::new();

    for (name, value) in &response.headers {
        // 予約ヘッダーはユーザー指定を無視
        if name.eq_ignore_ascii_case("Status") || name.eq_ignore_ascii_case("Content-Length") {
            continue;
        }
        if !is_valid_header_name(name) || !is_valid_header_value(value) {
            error!("Invalid header detected - name: '{}', value: '{}'", name, value);
            log_error_to_file(&format!(
                "CRLF injection attempt detected in header: '{}': '{}'",
                name, value
            ));
            // 安全な400レスポンスを構築
            response = Response::new(400)
                .with_header("Content-Type", "text/plain; charset=utf-8")
                .with_body(b"Bad Request: Invalid header".to_vec());
            sanitized_headers.clear();
            break;
        }
        sanitized_headers.push((name.clone(), value.clone()));
    }

    // ステータスコードとReason Phraseを準備
    let reason_phrase = match response.status {
        200 => "OK",
        201 => "Created",
        204 => "No Content",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        413 => "Payload Too Large",
        500 => "Internal Server Error",
        _ => "Unknown",
    };

    // ステータス行（CRLF）
    out.write_all(format!("Status: {} {}\r\n", response.status, reason_phrase).as_bytes())
        .map_err(|e| Error::InternalServerError(format!("Failed to write status line: {}", e)))?;

    // Set-Cookie を複数行で正しく出力するために振り分ける
    let mut normal_headers: Vec<(String, String)> = Vec::new();
    let mut set_cookie_values: Vec<String> = Vec::new();

    for (name, value) in sanitized_headers {
        if name.eq_ignore_ascii_case("Set-Cookie") {
            // 複数Cookieが1ヘッダーに連結されていた場合を安全に分割
            let parts = split_set_cookie_header(&value);
            if parts.is_empty() {
                // 分割できない（単一）場合はそのまま扱う
                set_cookie_values.push(value);
            } else {
                set_cookie_values.extend(parts);
            }
        } else {
            normal_headers.push((name, value));
        }
    }

    // 通常ヘッダーを出力
    for (name, value) in normal_headers {
        out.write_all(format!("{}: {}\r\n", name, value).as_bytes()).map_err(|e| {
            Error::InternalServerError(format!("Failed to write header: {}", e))
        })?;
    }

    // Set-Cookie を複数行で出力
    for cookie in set_cookie_values {
        out.write_all(format!("Set-Cookie: {}\r\n", cookie).as_bytes()).map_err(|e| {
            Error::InternalServerError(format!("Failed to write Set-Cookie header: {}", e))
        })?;
    }

    // Content-Length をフレームワーク側で付与（ボディがある場合）
    if let Some(body) = &response.body {
        out.write_all(format!("Content-Length: {}\r\n", body.len()).as_bytes()).map_err(|e| {
            Error::InternalServerError(format!("Failed to write Content-Length: {}", e))
        })?;
    }

    // ヘッダーとボディの区切り（CRLF）
    out.write_all(b"\r\n").map_err(|e| {
        Error::InternalServerError(format!("Failed to write header/body separator: {}", e))
    })?;

    // ボディ出力
    if let Some(body) = response.body {
        out.write_all(&body).map_err(|e| {
            Error::InternalServerError(format!("Failed to write response body: {}", e))
        })?;
    }

    Ok(())
}

/// レスポンスを標準出力に書き出す
pub fn write_response(response: Response) -> Result<(), Error> {
    let mut out = io::stdout().lock();
    let res = write_response_to(response, &mut out);
    out.flush().map_err(|e| Error::InternalServerError(format!("Failed to flush stdout: {}", e)))?;
    res
}

/// 連結された Set-Cookie ヘッダー値を安全に分割する
/// 注意: RFC的にはSet-Cookieは結合不可だが、実装上HashMap制約の回避として
/// "," 区切りで結合されたケースを考慮し、Expires 属性内のカンマは分割対象から除外する。
pub fn split_set_cookie_header(value: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut buf = String::new();
    let mut in_expires = false;
    let mut chars = value.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            // セミコロンで属性の区切りを検出（Expires= のスコープ終端にもなる）
            ';' => {
                in_expires = false; // Expires= の属性スコープを抜ける
                buf.push(ch);
            }
            // カンマは、Expires= 属性中ならそのまま、それ以外ならCookie間区切りの可能性
            ',' => {
                if in_expires {
                    buf.push(ch);
                } else {
                    // 直後の空白をスキップ
                    while let Some(' ') = chars.peek() {
                        chars.next();
                    }
                    // 次のトークンが cookie-pair らしい（= を含む）なら分割、それ以外は文字として扱う
                    // 先読みして '=' がセミコロンより前に現れるかを確認
                    let mut lookahead = String::new();
                    let mut iter = chars.clone();
                    let mut seen_eq_before_semicolon = false;
                    while let Some(&c) = iter.peek() {
                        if c == ';' || c == ',' { break; }
                        if c == '=' { seen_eq_before_semicolon = true; break; }
                        lookahead.push(c);
                        iter.next();
                    }
                    if seen_eq_before_semicolon {
                        // ここで一旦Cookieを確定
                        let part = buf.trim();
                        if !part.is_empty() { result.push(part.to_string()); }
                        buf.clear();
                        continue;
                    } else {
                        // Cookie間区切りではないので文字として追加
                        buf.push(',');
                    }
                }
            }
            // 'E' または 'e' から始まる Expires= を検出してフラグを立てる
            'E' | 'e' => {
                // 現在位置から "xpires=" までを確認（ケースインセンシティブ）
                let mut shadow = chars.clone();
                let mut matches = true;
                for expected in ['x','p','i','r','e','s','='] {
                    if let Some(c) = shadow.next() {
                        if c.to_ascii_lowercase() != expected { matches = false; break; }
                    } else { matches = false; break; }
                }
                if matches {
                    in_expires = true;
                }
                buf.push(ch);
            }
            _ => {
                buf.push(ch);
            }
        }
    }

    let tail = buf.trim();
    if !tail.is_empty() {
        result.push(tail.to_string());
    }

    // 単一Cookieしか得られなかった場合は、
    // 呼び出し側でそのまま扱えるように空ベクタではなく単一要素でも返す
    result
}