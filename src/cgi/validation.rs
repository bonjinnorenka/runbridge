//! ヘッダーとデータの検証機能

/// ヘッダー名が安全かどうか検証する（ASCII英数+ハイフンのみ）
pub fn is_valid_header_name(name: &str) -> bool {
    let b = name.as_bytes();
    if b.is_empty() {
        return false;
    }
    // 許可: A-Z a-z 0-9 '-'
    if !b.iter().all(|&c| c.is_ascii_alphanumeric() || c == b'-') {
        return false;
    }
    true
}

/// ヘッダー値が安全かどうか検証する（ASCIIのホワイトリスト）
/// 許可: HTAB(0x09), SP(0x20), 可視ASCII(0x21–0x7E)
pub fn is_valid_header_value(value: &str) -> bool {
    value
        .as_bytes()
        .iter()
        .all(|&c| c == b'\t' || c == b' ' || (0x21..=0x7e).contains(&c))
}