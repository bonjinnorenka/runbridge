use log::warn;
use crate::error::Error;

/// パターンの安全性を確保（アンカーの確認と追加）
pub fn ensure_safe_pattern(pattern: &str) -> Result<String, Error> {
    if pattern.is_empty() {
        return Err(Error::InvalidRequestBody("Empty regex pattern is not allowed".to_string()));
    }

    let has_start_anchor = pattern.starts_with('^');
    let has_end_anchor = pattern.ends_with('$');

    if !has_start_anchor || !has_end_anchor {
        let safe_pattern = format!(
            "^{}$",
            pattern.trim_start_matches('^').trim_end_matches('$')
        );
        warn!(
            "Pattern '{}' lacks proper anchors, converted to '{}' for security",
            pattern,
            safe_pattern
        );
        Ok(safe_pattern)
    } else {
        Ok(pattern.to_string())
    }
}

