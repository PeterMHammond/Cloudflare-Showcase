use worker::*;
use worker::console_log;
use wasm_bindgen::prelude::*;
use serde::{Deserialize, Serialize};
use crate::utils::sql_bindings::SqlStorageExt;

#[derive(Serialize, Deserialize, Debug)]
struct Message {
    id: Option<i64>,
    timestamp: u64,
    content: String,
    user_id: String,
}

#[wasm_bindgen]
pub struct ExampleSqliteDO {
    state: State,
    env: Env,
    initialized: bool,
}

impl ExampleSqliteDO {
    async fn init_database(&self) -> Result<()> {
        let storage = self.state.storage();
        let sql = storage.sql()?;
        
        // Create tables if they don't exist
        sql.exec(r#"
            CREATE TABLE IF NOT EXISTS messages (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp INTEGER NOT NULL,
                content TEXT NOT NULL,
                user_id TEXT NOT NULL
            )
        "#)?;
        
        sql.exec(r#"
            CREATE INDEX IF NOT EXISTS idx_timestamp ON messages(timestamp);
            CREATE INDEX IF NOT EXISTS idx_user_id ON messages(user_id);
        "#)?;
        
        console_log!("Database initialized successfully");
        Ok(())
    }
    
    async fn add_message(&self, content: String, user_id: String) -> Result<Message> {
        let storage = self.state.storage();
        let sql = storage.sql()?;
        let timestamp = Date::now().as_millis();
        
        let stmt = sql.prepare("INSERT INTO messages (timestamp, content, user_id) VALUES (?, ?, ?)")?;
        let meta = stmt.run_with_params(&js_sys::Array::of3(
            &JsValue::from(timestamp),
            &JsValue::from(content.clone()),
            &JsValue::from(user_id.clone())
        ))?;
        
        // Get the last inserted row ID using SQL
        let cursor = sql.exec("SELECT last_insert_rowid() as id")?;
        let result = cursor.toArray();
        let id = if result.length() > 0 {
            let row = result.get(0);
            js_sys::Reflect::get(&row, &JsValue::from_str("id"))
                .ok()
                .and_then(|val| val.as_f64())
                .map(|f| f as i64)
                .unwrap_or(0)
        } else {
            0
        };
        
        Ok(Message {
            id: Some(id),
            timestamp,
            content,
            user_id,
        })
    }
    
    async fn get_recent_messages(&self, limit: u32) -> Result<Vec<Message>> {
        let storage = self.state.storage();
        let sql = storage.sql()?;
        
        // Using prepared statement with parameter
        let stmt = sql.prepare("SELECT * FROM messages ORDER BY timestamp DESC LIMIT ?")?;
        let cursor = stmt.bind_with_params(&js_sys::Array::of1(&JsValue::from(limit)))?;
        
        cursor.collect()
    }
    
    async fn get_user_messages(&self, user_id: &str) -> Result<Vec<Message>> {
        let storage = self.state.storage();
        let sql = storage.sql()?;
        
        let stmt = sql.prepare("SELECT * FROM messages WHERE user_id = ? ORDER BY timestamp DESC")?;
        let cursor = stmt.bind_values(&[user_id])?;
        
        cursor.collect()
    }
    
    async fn delete_old_messages(&self, hours: u32) -> Result<u64> {
        let storage = self.state.storage();
        let sql = storage.sql()?;
        
        let cutoff_time = Date::now().as_millis() - (hours as u64 * 60 * 60 * 1000);
        
        let stmt = sql.prepare("DELETE FROM messages WHERE timestamp < ?")?;
        let meta = stmt.run_with_params(&js_sys::Array::of1(&JsValue::from(cutoff_time)))?;
        
        Ok(meta.rows_written())
    }
    
    async fn export_database(&self) -> Result<Vec<u8>> {
        let storage = self.state.storage();
        let sql = storage.sql()?;
        sql.dump().map_err(|e| Error::JsError(format!("Failed to dump database: {:?}", e)))
    }
    
    async fn get_statistics(&self) -> Result<serde_json::Value> {
        let storage = self.state.storage();
        let sql = storage.sql()?;
        
        let cursor = sql.exec(r#"
            SELECT 
                COUNT(*) as total_messages,
                COUNT(DISTINCT user_id) as unique_users,
                MIN(timestamp) as first_message_time,
                MAX(timestamp) as last_message_time
            FROM messages
        "#)?;
        
        let stats = cursor.toArray();
        if stats.length() > 0 {
            Ok(serde_wasm_bindgen::from_value(stats.get(0))?)
        } else {
            Ok(serde_json::json!({
                "total_messages": 0,
                "unique_users": 0,
                "first_message_time": null,
                "last_message_time": null
            }))
        }
    }
}

#[durable_object]
impl DurableObject for ExampleSqliteDO {
    fn new(state: State, env: Env) -> Self {
        Self {
            state,
            env,
            initialized: false,
        }
    }
    
    async fn fetch(&mut self, mut req: Request) -> Result<Response> {
        if !self.initialized {
            self.init_database().await?;
            self.initialized = true;
        }
        
        let url = req.url()?;
        let path = url.path();
        console_log!("SQLite DO received request: {} {}", req.method(), path);
        
        // Strip the /sqlite/api prefix from the path
        let api_path = path.strip_prefix("/sqlite/api").unwrap_or(path);
        console_log!("Stripped path: '{}' -> '{}'", path, api_path);
        
        // Now also log what we're matching against
        console_log!("Attempting to match: method={:?}, api_path='{}'", req.method(), api_path);
        
        match (req.method(), api_path) {
            (Method::Post, "/message") => {
                #[derive(Deserialize)]
                struct PostMessage {
                    content: String,
                    user_id: String,
                }
                
                let body: PostMessage = req.json().await.map_err(|e| Error::RustError(format!("Failed to parse JSON: {}", e)))?;
                let message = self.add_message(body.content, body.user_id).await?;
                
                console_log!("Message stored with id: {:?}", message.id);
                
                Response::from_json(&message)
            }
            
            (Method::Get, "/messages") => {
                let query_string = url.query().unwrap_or_default();
                let query_params: std::collections::HashMap<String, String> = query_string
                    .split('&')
                    .filter(|s| !s.is_empty())
                    .filter_map(|pair| {
                        let mut parts = pair.split('=');
                        match (parts.next(), parts.next()) {
                            (Some(key), Some(value)) => Some((key.to_string(), value.to_string())),
                            _ => None,
                        }
                    })
                    .collect();
                
                let limit = query_params.get("limit")
                    .and_then(|l| l.parse().ok())
                    .unwrap_or(50);
                    
                let messages = self.get_recent_messages(limit).await?;
                Response::from_json(&messages)
            }
            
            (Method::Get, user_path) if user_path.starts_with("/user/") => {
                let user_id = user_path.trim_start_matches("/user/");
                let messages = self.get_user_messages(user_id).await?;
                Response::from_json(&messages)
            }
            
            (Method::Delete, "/old") => {
                console_log!("Processing DELETE /old request");
                let query_string = url.query().unwrap_or_default();
                console_log!("Query string: '{}'", query_string);
                
                let query_params: std::collections::HashMap<String, String> = query_string
                    .split('&')
                    .filter(|s| !s.is_empty())
                    .filter_map(|pair| {
                        let mut parts = pair.split('=');
                        match (parts.next(), parts.next()) {
                            (Some(key), Some(value)) => Some((key.to_string(), value.to_string())),
                            _ => None,
                        }
                    })
                    .collect();
                
                console_log!("Parsed query params: {:?}", query_params);
                
                let hours = query_params.get("hours")
                    .and_then(|h| h.parse().ok())
                    .unwrap_or(24);
                    
                console_log!("Deleting messages older than {} hours", hours);
                let deleted = self.delete_old_messages(hours).await?;
                console_log!("Deleted {} messages", deleted);
                
                Response::from_json(&serde_json::json!({
                    "deleted": deleted
                }))
            }
            
            (Method::Get, "/stats") => {
                let stats = self.get_statistics().await?;
                Response::from_json(&stats)
            }
            
            (Method::Get, "/export") => {
                let dump = self.export_database().await?;
                let mut response = Response::from_bytes(dump)?;
                response.headers_mut().set("Content-Type", "application/octet-stream")?;
                response.headers_mut().set("Content-Disposition", "attachment; filename=\"database.sqlite\"")?;
                Ok(response)
            }
            
            _ => {
                console_log!("No match found for: method={:?}, api_path='{}', full_path='{}']", req.method(), api_path, path);
                Response::error(format!("Not Found: {} {}", req.method(), api_path), 404)
            }
        }
    }
}

pub async fn handler(req: Request, ctx: RouteContext<crate::utils::middleware::ValidationState>) -> Result<Response> {
    let namespace = ctx.env.durable_object("ExampleSqliteDO")?;
    let stub = namespace.id_from_name("sqlite-demo-instance")?.get_stub()?;
    stub.fetch_with_request(req).await
}