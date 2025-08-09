//! リクエストコンテキストの実装

use std::collections::HashMap;
use std::any::Any;

/// リクエストコンテキスト（ミドルウェア間でのデータ共有）
#[derive(Debug, Default)]
pub struct RequestContext {
    metadata: HashMap<String, Box<dyn Any + Send + Sync>>,
}

impl RequestContext {
    /// 新しいRequestContextを作成
    pub fn new() -> Self {
        Self {
            metadata: HashMap::new(),
        }
    }

    /// 値を設定
    pub fn set<T: Send + Sync + 'static>(&mut self, key: &str, value: T) {
        self.metadata.insert(key.to_string(), Box::new(value));
    }

    /// 値を取得
    pub fn get<T: 'static>(&self, key: &str) -> Option<&T> {
        self.metadata
            .get(key)
            .and_then(|boxed| boxed.downcast_ref::<T>())
    }

    /// 値を削除して返却
    pub fn remove<T: 'static>(&mut self, key: &str) -> Option<T> {
        self.metadata
            .remove(key)
            .and_then(|boxed| boxed.downcast::<T>().ok())
            .map(|boxed| *boxed)
    }

    /// 指定されたキーが存在するかチェック
    pub fn contains_key(&self, key: &str) -> bool {
        self.metadata.contains_key(key)
    }

    /// 全てのキーを取得
    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.metadata.keys()
    }

    /// コンテキストをクリア
    pub fn clear(&mut self) {
        self.metadata.clear();
    }

    /// コンテキストが空かどうか
    pub fn is_empty(&self) -> bool {
        self.metadata.is_empty()
    }
}

impl Clone for RequestContext {
    fn clone(&self) -> Self {
        // Anyトレイトはcloneをサポートしていないため、新しい空のコンテキストを作成
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_context_basic() {
        let mut context = RequestContext::new();

        // 値の設定と取得
        context.set("string_val", "hello".to_string());
        context.set("int_val", 42i32);
        context.set("bool_val", true);

        assert_eq!(context.get::<String>("string_val"), Some(&"hello".to_string()));
        assert_eq!(context.get::<i32>("int_val"), Some(&42));
        assert_eq!(context.get::<bool>("bool_val"), Some(&true));

        // 存在しないキー
        assert_eq!(context.get::<String>("nonexistent"), None);

        // 間違った型
        assert_eq!(context.get::<i32>("string_val"), None);
    }

    #[test]
    fn test_request_context_contains_and_keys() {
        let mut context = RequestContext::new();
        
        assert!(context.is_empty());
        assert!(!context.contains_key("test"));

        context.set("key1", "value1".to_string());
        context.set("key2", 123);

        assert!(!context.is_empty());
        assert!(context.contains_key("key1"));
        assert!(context.contains_key("key2"));
        assert!(!context.contains_key("key3"));

        let keys: Vec<&String> = context.keys().collect();
        assert_eq!(keys.len(), 2);
        assert!(keys.contains(&&"key1".to_string()));
        assert!(keys.contains(&&"key2".to_string()));
    }

    #[test]
    fn test_request_context_remove() {
        let mut context = RequestContext::new();
        
        context.set("removable", "test_value".to_string());
        assert!(context.contains_key("removable"));

        let removed: Option<String> = context.remove("removable");
        assert_eq!(removed, Some("test_value".to_string()));
        assert!(!context.contains_key("removable"));

        // 既に削除済みのキー
        let removed: Option<String> = context.remove("removable");
        assert_eq!(removed, None);
    }

    #[test]
    fn test_request_context_clear() {
        let mut context = RequestContext::new();
        
        context.set("key1", "value1".to_string());
        context.set("key2", 42);
        assert!(!context.is_empty());

        context.clear();
        assert!(context.is_empty());
        assert!(!context.contains_key("key1"));
        assert!(!context.contains_key("key2"));
    }

    #[derive(Debug, Clone, PartialEq)]
    struct UserInfo {
        id: u32,
        name: String,
    }

    #[test]
    fn test_request_context_custom_types() {
        let mut context = RequestContext::new();
        
        let user = UserInfo { id: 42, name: "Alice".to_string() };
        context.set("user", user.clone());

        let retrieved_user = context.get::<UserInfo>("user");
        assert_eq!(retrieved_user, Some(&user));

        let removed_user: Option<UserInfo> = context.remove("user");
        assert_eq!(removed_user, Some(user));
    }
}