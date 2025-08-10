//! 共通ユーティリティ関数群（URLデコード、クエリ解析、環境設定 等）

use std::collections::HashMap;
use std::env;
use crate::error::Error;

/// URLエンコーディングのデコード関数
pub fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut result = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (from_hex(bytes[i + 1]), from_hex(bytes[i + 2])) {
                result.push(h * 16 + l);
                i += 3;
                continue;
            }
        } else if bytes[i] == b'+' {
            result.push(b' ');
            i += 1;
            continue;
        }
        result.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&result).into_owned()
}

/// 16進数文字をバイト値に変換するヘルパー関数
fn from_hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// クエリ文字列をパースしてURLデコードを行う共通関数
pub fn parse_query_string(query_string: &str) -> HashMap<String, String> {
    let mut params = HashMap::new();

    if query_string.is_empty() {
        return params;
    }

    for pair in query_string.split('&') {
        let mut parts = pair.splitn(2, '=');
        if let Some(key) = parts.next() {
            let value = parts.next().unwrap_or("");
            let decoded_key = percent_decode(key);
            let decoded_value = percent_decode(value);
            params.insert(decoded_key, decoded_value);
        }
    }

    params
}

/// リクエストボディの最大サイズ（バイト）を取得する
/// 優先順位: 環境変数 `RUNBRIDGE_MAX_BODY_SIZE` -> デフォルト 5MB
pub fn get_max_body_size() -> usize {
    const DEFAULT_MAX_SIZE: usize = 5 * 1024 * 1024; // 5MB
    env::var("RUNBRIDGE_MAX_BODY_SIZE")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(DEFAULT_MAX_SIZE)
}

/// ヘッダー値に使用可能な文字かを判定（CRLF・制御文字を拒否）
pub fn is_header_value_valid(value: &str) -> bool {
    // RFC的にはobs-text等もありうるが、ここでは保守的にUS-ASCII可視範囲に限定し、
    // 制御文字(0x00-0x1F, 0x7F)およびCR/LFを拒否する
    if value.is_empty() {
        return true; // 空は許容（ヘッダー仕様上も可）
    }
    value.chars().all(|c| {
        let code = c as u32;
        code >= 0x20 && code != 0x7F && c != '\r' && c != '\n'
    })
}

/// ヘッダー名が安全なトークンかを簡易判定（使わないが将来拡張用）
#[allow(dead_code)]
pub fn is_header_name_valid(name: &str) -> bool {
    if name.is_empty() { return false; }
    // token = 1*tchar, tchar = "!#$%&'*+-.^_`|~" or DIGIT or ALPHA
    name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '!'|'#'|'$'|'%'|'&'|'\''|'*'|'+'|'-'|'.'|'^'|'_'|'`'|'|'|'~'))
}

/// Cookie名が安全なトークンか（RFC6265準拠の簡易版）
pub fn is_cookie_name_valid(name: &str) -> bool {
    if name.is_empty() { return false; }
    // tokenと同等: 制御/空白とセパレータを除外
    const FORBIDDEN: &[char] = &['(',')','<','>','@',',',';',':','\\','"','/','[',']','?','{','}',' ','\t','\r','\n'];
    name.chars().all(|c| c.is_ascii() && !c.is_ascii_control() && !FORBIDDEN.contains(&c))
}

/// Cookie値が安全か（RFC6265 cookie-octetの簡易版）
/// 許容: 0x21, 0x23-0x2B, 0x2D-0x3A, 0x3C-0x5B, 0x5D-0x7E
pub fn is_cookie_value_valid(value: &str) -> bool {
    value.chars().all(|c| {
        let b = c as u32;
        matches!(b,
            0x21 |
            0x23..=0x2B |
            0x2D..=0x3A |
            0x3C..=0x5B |
            0x5D..=0x7E
        )
    })
}

/// ヘルパー: 無効なヘッダー値ならErrorを返す
pub fn validate_header_value(value: &str) -> Result<(), Error> {
    if is_header_value_valid(value) { Ok(()) } else { Err(Error::InvalidHeader("header value contains control/CRLF or invalid chars".into())) }
}

/// ヘルパー: 無効なCookie名/値ならErrorを返す
pub fn validate_cookie_name_value(name: &str, value: &str) -> Result<(), Error> {
    if !is_cookie_name_valid(name) {
        return Err(Error::InvalidCookie("cookie name contains invalid characters".into()));
    }
    if !is_cookie_value_valid(value) {
        return Err(Error::InvalidCookie("cookie value contains invalid characters".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_query_string() {
        let query = "name=John&age=30&city=Tokyo";
        let params = parse_query_string(query);
        
        assert_eq!(params.get("name"), Some(&"John".to_string()));
        assert_eq!(params.get("age"), Some(&"30".to_string()));
        assert_eq!(params.get("city"), Some(&"Tokyo".to_string()));
    }

    #[test]
    fn test_parse_query_string_url_encoding() {
        // URLエンコードされたクエリ文字列
        let query = "name=%E3%81%82%E3%81%84%E3%81%86%E3%81%88%E3%81%8A&city=Tokyo%20Station&lang=ja%2Den";
        let params = parse_query_string(query);

        // "あいうえお"（UTF-8でURLエンコード）
        assert_eq!(params.get("name"), Some(&"あいうえお".to_string()));
        // スペースが%20でエンコードされている
        assert_eq!(params.get("city"), Some(&"Tokyo Station".to_string()));
        // ハイフンが%2Dでエンコードされている
        assert_eq!(params.get("lang"), Some(&"ja-en".to_string()));
    }

    #[test]
    fn test_percent_decode() {
        assert_eq!(percent_decode("Hello%20World"), "Hello World");
        assert_eq!(percent_decode("test%2Bvalue"), "test+value");
        assert_eq!(percent_decode("normal"), "normal");
        assert_eq!(percent_decode("plus+space"), "plus space"); // +もスペースに変換
        assert_eq!(percent_decode("%E3%81%82%E3%81%84%E3%81%86%E3%81%88%E3%81%8A"), "あいうえお");
    }
}

#[cfg(test)]
mod sec_tests {
    use super::*;

    #[test]
    fn header_value_rejects_crlf_and_ctl() {
        assert!(is_header_value_valid("normal-Value_123"));
        assert!(!is_header_value_valid("bad\rvalue"));
        assert!(!is_header_value_valid("bad\nvalue"));
        assert!(!is_header_value_valid("bad\x07bell"));
    }

    #[test]
    fn cookie_name_and_value_validation() {
        assert!(is_cookie_name_valid("SESSIONID"));
        assert!(!is_cookie_name_valid("bad name"));
        assert!(!is_cookie_name_valid("bad;name"));

        assert!(is_cookie_value_valid("abcDEF123-_.:~"));
        assert!(!is_cookie_value_valid("bad;value"));
        assert!(!is_cookie_value_valid("bad,value"));
        assert!(!is_cookie_value_valid("bad\nvalue"));
    }
}
