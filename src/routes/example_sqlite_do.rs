use worker::*;
use worker::console_log;
use wasm_bindgen::prelude::*;
use serde::{Deserialize, Serialize};
use crate::utils::sql_bindings::{SqlStorageExt, SqlStorage};
use crate::sql_query;

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
        
        Ok(Message {
            id: Some(meta.rows_written() as i64),
            timestamp,
            content,
            user_id,
        })
    }
    
    async fn get_recent_messages(&self, limit: u32) -> Result<Vec<Message>> {
        let storage = self.state.storage();
        let sql = storage.sql()?;
        
        // Using the macro for convenience
        let cursor = sql_query!(storage, 
            "SELECT * FROM messages ORDER BY timestamp DESC LIMIT ?", 
            limit
        )?;
        
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
        sql.dump()
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
    
    async fn fetch(&mut self, req: Request) -> Result<Response> {
        if !self.initialized {
            self.init_database().await?;
            self.initialized = true;
        }
        
        let url = req.url()?;
        let path = url.path();
        
        match (req.method(), path) {
            (Method::Post, "/message") => {
                #[derive(Deserialize)]
                struct PostMessage {
                    content: String,
                    user_id: String,
                }
                
                let body: PostMessage = req.json().await?;
                let message = self.add_message(body.content, body.user_id).await?;
                Response::from_json(&message)
            }
            
            (Method::Get, "/messages") => {
                let limit = url.search_params()
                    .get("limit")
                    .and_then(|l| l.parse().ok())
                    .unwrap_or(50);
                    
                let messages = self.get_recent_messages(limit).await?;
                Response::from_json(&messages)
            }
            
            (Method::Get, path) if path.starts_with("/user/") => {
                let user_id = path.trim_start_matches("/user/");
                let messages = self.get_user_messages(user_id).await?;
                Response::from_json(&messages)
            }
            
            (Method::Delete, "/old") => {
                let hours = url.search_params()
                    .get("hours")
                    .and_then(|h| h.parse().ok())
                    .unwrap_or(24);
                    
                let deleted = self.delete_old_messages(hours).await?;
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
            
            _ => Response::error("Not Found", 404)
        }
    }
}

pub async fn handler(req: Request, ctx: RouteContext<crate::utils::middleware::ValidationState>) -> Result<Response> {
    let namespace = ctx.env.durable_object("ExampleSqliteDO")?;
    let stub = namespace.id_from_name("ExampleSqliteDO")?.get_stub()?;
    stub.fetch_with_request(req).await
}