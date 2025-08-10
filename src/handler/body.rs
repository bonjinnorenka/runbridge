/// Content-Typeの許容範囲を判定（拡張しやすい実装）
pub fn is_json_like_content_type(ct: &str) -> bool {
    let main_type = ct
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();

    // 明示リスト（将来拡張しやすい）
    const EXTRA_ALLOWED: &[&str] = &[
        // RFC 7464 JSON Text Sequences（ボディ仕様は異なるが、将来的拡張を想定）
        "application/json-seq",
    ];

    main_type == "application/json"
        || main_type.ends_with("+json")
        || EXTRA_ALLOWED.contains(&main_type.as_str())
}

