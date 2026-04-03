//! リクエストコンテキストの実装

use std::any::Any;
use std::collections::HashMap;
use std::sync::Arc;

/// リクエストコンテキスト（ミドルウェア間でのデータ共有）
#[derive(Debug, Default)]
pub struct RequestContext {
    metadata: HashMap<String, Box<dyn Any + Send + Sync>>,
    app_state: Option<Arc<dyn Any + Send + Sync>>,
}

impl RequestContext {
    /// 新しいRequestContextを作成
    pub fn new() -> Self {
        Self {
            metadata: HashMap::new(),
            app_state: None,
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

    /// アプリケーション全体の共有 state を設定
    pub fn set_app_state(&mut self, state: Option<Arc<dyn Any + Send + Sync>>) {
        self.app_state = state;
    }

    /// アプリケーション全体の共有 state を取得
    pub fn app_state<T: Send + Sync + 'static>(&self) -> Option<Arc<T>> {
        self.app_state
            .as_ref()
            .and_then(|state| Arc::clone(state).downcast::<T>().ok())
    }
}

impl RequestContext {
    /// 新しい空のコンテキストを作成（共有 state は保持）
    pub fn clone_empty(&self) -> Self {
        Self {
            metadata: HashMap::new(),
            app_state: self.app_state.clone(),
        }
    }

    /// 可能な場合にディープコピーを試行（現在はメタデータを複製しない）
    pub fn try_clone(&self) -> Self {
        #[cfg(debug_assertions)]
        log::debug!(
            "RequestContext::try_clone() called - returning empty metadata due to Any trait limitations"
        );
        self.clone_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_context_basic() {
        let mut context = RequestContext::new();

        context.set("string_val", "hello".to_string());
        context.set("int_val", 42i32);
        context.set("bool_val", true);

        assert_eq!(
            context.get::<String>("string_val"),
            Some(&"hello".to_string())
        );
        assert_eq!(context.get::<i32>("int_val"), Some(&42));
        assert_eq!(context.get::<bool>("bool_val"), Some(&true));
        assert_eq!(context.get::<String>("nonexistent"), None);
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

        let user = UserInfo {
            id: 42,
            name: "Alice".to_string(),
        };
        context.set("user", user.clone());

        let retrieved_user = context.get::<UserInfo>("user");
        assert_eq!(retrieved_user, Some(&user));

        let removed_user: Option<UserInfo> = context.remove("user");
        assert_eq!(removed_user, Some(user));
    }

    #[test]
    fn test_request_context_safe_cloning() {
        let mut context = RequestContext::new();
        context.set("key1", "value1".to_string());
        context.set("key2", 42i32);

        let empty_clone = context.clone_empty();
        assert!(empty_clone.is_empty());
        assert!(!empty_clone.contains_key("key1"));
        assert!(!empty_clone.contains_key("key2"));

        let try_clone = context.try_clone();
        assert!(try_clone.is_empty());
        assert!(!try_clone.contains_key("key1"));
        assert!(!try_clone.contains_key("key2"));

        assert!(!context.is_empty());
        assert!(context.contains_key("key1"));
        assert!(context.contains_key("key2"));
    }

    #[test]
    fn test_request_context_app_state() {
        let mut context = RequestContext::new();
        context.set_app_state(Some(Arc::new(123usize)));

        let state = context.app_state::<usize>().unwrap();
        assert_eq!(*state, 123);

        let empty_clone = context.clone_empty();
        let state = empty_clone.app_state::<usize>().unwrap();
        assert_eq!(*state, 123);
    }
}
