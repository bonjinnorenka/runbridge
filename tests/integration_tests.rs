//! インテグレーションテスト

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use serde::{Serialize, Deserialize};
    use runbridge::{RunBridge, common::{Request, Response, Method}, handler, error::Error};

    // テスト用のデータ構造
    #[derive(Serialize, Deserialize, Debug, PartialEq)]
    struct ItemRequest {
        name: String,
        description: Option<String>,
    }

    #[derive(Serialize, Deserialize, Debug, PartialEq)]
    struct ItemResponse {
        id: String,
        name: String,
        description: Option<String>,
        created_at: String,
    }

    // GET ハンドラー
    fn get_item_handler(req: Request) -> Result<ItemResponse, Error> {
        // パスからIDを抽出 (例: /items/123 -> 123)
        let path_parts: Vec<&str> = req.path.split('/').collect();
        let id = path_parts.last().unwrap_or(&"unknown").to_string();

        Ok(ItemResponse {
            id,
            name: "Test Item".to_string(),
            description: Some("This is a test item".to_string()),
            created_at: "2023-01-01T00:00:00Z".to_string(),
        })
    }

    // POST ハンドラー
    fn create_item_handler(_req: Request, item: ItemRequest) -> Result<ItemResponse, Error> {
        Ok(ItemResponse {
            id: "new_item_123".to_string(),
            name: item.name,
            description: item.description,
            created_at: "2023-01-01T00:00:00Z".to_string(),
        })
    }

    #[tokio::test]
    async fn test_app_routing() {
        // アプリケーションの構築
        let app = RunBridge::builder()
            .handler(handler::get(r"^/items/[^/]+$", get_item_handler))
            .handler(handler::post("/items", create_item_handler))
            .build();

        // GETリクエストのテスト
        let get_req = Request::new(Method::GET, "/items/123".to_string());
        let handler = app.find_handler(&get_req.path, &get_req.method).expect("Handler not found");
        let get_result = handler.handle(get_req).await.expect("Handler failed");

        assert_eq!(get_result.status, 200);
        let body_str = String::from_utf8(get_result.body.unwrap()).unwrap();
        let response: ItemResponse = serde_json::from_str(&body_str).unwrap();
        assert_eq!(response.id, "123");

        // POSTリクエストのテスト
        let req_data = ItemRequest {
            name: "New Item".to_string(),
            description: Some("This is a new item".to_string()),
        };
        let json_body = serde_json::to_vec(&req_data).unwrap();
        let post_req = Request::new(Method::POST, "/items".to_string())
            .with_header("Content-Type", "application/json")
            .with_body(json_body);

        let handler = app.find_handler(&post_req.path, &post_req.method).expect("Handler not found");
        let post_result = handler.handle(post_req).await.expect("Handler failed");

        assert_eq!(post_result.status, 200);
        let body_str = String::from_utf8(post_result.body.unwrap()).unwrap();
        let response: ItemResponse = serde_json::from_str(&body_str).unwrap();
        assert_eq!(response.name, "New Item");
        assert_eq!(response.id, "new_item_123");
    }

    #[tokio::test]
    async fn test_nonexistent_route() {
        // アプリケーションの構築
        let app = RunBridge::builder()
            .handler(handler::get("/items", get_item_handler))
            .build();

        // 存在しないパスへのリクエスト
        let req = Request::new(Method::GET, "/nonexistent".to_string());
        let handler = app.find_handler(&req.path, &req.method);
        
        assert!(handler.is_none(), "Handler should not be found for nonexistent path");
    }

    // ミドルウェアのテスト
    struct TestMiddleware {
        name: String,
    }

    #[async_trait::async_trait]
    impl runbridge::common::Middleware for TestMiddleware {
        async fn pre_process(&self, mut req: Request) -> Result<Request, Error> {
            // ヘッダーを追加
            req.headers.insert("X-Middleware".to_string(), self.name.clone());
            Ok(req)
        }

        async fn post_process(&self, mut res: Response) -> Result<Response, Error> {
            // ヘッダーを追加
            res.headers.insert("X-Middleware-Response".to_string(), self.name.clone());
            Ok(res)
        }
    }

    #[tokio::test]
    async fn test_middleware() {
        // ミドルウェア付きのアプリケーションを構築
        let app = RunBridge::builder()
            .middleware(TestMiddleware { name: "Test1".to_string() })
            .middleware(TestMiddleware { name: "Test2".to_string() })
            .handler(handler::get("/test", |_| Ok("Test Response")))
            .build();

        // リクエストの作成
        let req = Request::new(Method::GET, "/test".to_string());
        
        // ハンドラーの取得と実行
        let handler = app.find_handler(&req.path, &req.method).expect("Handler not found");
        
        // リクエスト前処理（ミドルウェア適用）
        let mut req_processed = req;
        for middleware in app.middlewares() {
            req_processed = middleware.pre_process(req_processed).await.unwrap();
        }
        
        // ハンドラー実行
        let mut response = handler.handle(req_processed).await.unwrap();
        
        // レスポンス後処理（ミドルウェア適用）
        for middleware in app.middlewares() {
            response = middleware.post_process(response).await.unwrap();
        }
        
        // ミドルウェアが適切に適用されたか検証
        assert_eq!(response.headers.get("X-Middleware-Response").unwrap(), "Test2");
    }
} 
