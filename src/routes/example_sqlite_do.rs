use worker::*;
use worker::console_log;
use wasm_bindgen::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Serialize, Deserialize, Debug, Clone)]
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
    messages: HashMap<String, Message>,
    next_id: i64,
}

impl ExampleSqliteDO {
    async fn init_database(&self) -> Result<()> {
        console_log!("Simulating database initialization (SQLite not yet available in workers-rs)");
        Ok(())
    }
    
    async fn add_message(&mut self, content: String, user_id: String) -> Result<Message> {
        let timestamp = Date::now().as_millis();
        
        let message = Message {
            id: Some(self.next_id),
            timestamp,
            content,
            user_id,
        };
        
        console_log!("Simulating SQL: INSERT INTO messages (timestamp, content, user_id) VALUES ({}, '{}', '{}')", 
            timestamp, &message.content, &message.user_id);
        
        Ok(message)
    }
    
    async fn get_recent_messages(&self, _limit: u32) -> Result<Vec<Message>> {
        console_log!("Simulating SQL: SELECT * FROM messages ORDER BY timestamp DESC LIMIT {}", _limit);
        
        let mut messages: Vec<Message> = self.messages.values()
            .cloned()
            .collect();
        
        // Sort by timestamp descending (newest first)
        messages.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        
        // Take only the requested limit
        messages.truncate(_limit as usize);
        
        console_log!("Returning {} messages from total of {}", messages.len(), self.messages.len());
        
        Ok(messages)
    }
    
    async fn get_user_messages(&self, user_id: &str) -> Result<Vec<Message>> {
        console_log!("Simulating SQL: SELECT * FROM messages WHERE user_id = '{}' ORDER BY timestamp DESC", user_id);
        
        let mut messages: Vec<Message> = self.messages.values()
            .filter(|m| m.user_id == user_id)
            .cloned()
            .collect();
        
        // Sort by timestamp descending (newest first)
        messages.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        
        console_log!("Returning {} messages for user {} from total of {}", messages.len(), user_id, self.messages.len());
        
        Ok(messages)
    }
    
    async fn delete_old_messages(&mut self, hours: u32) -> Result<u64> {
        let cutoff_time = Date::now().as_millis() - (hours as u64 * 60 * 60 * 1000);
        
        console_log!("Simulating SQL: DELETE FROM messages WHERE timestamp < {}", cutoff_time);
        
        let count_before = self.messages.len();
        let messages_to_keep: HashMap<String, Message> = self.messages.iter()
            .filter(|(_, msg)| msg.timestamp >= cutoff_time)
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        
        self.messages = messages_to_keep;
        let deleted = (count_before - self.messages.len()) as u64;
        console_log!("Deleted {} messages, {} remaining", deleted, self.messages.len());
        
        Ok(deleted)
    }
    
    async fn export_database(&self) -> Result<Vec<u8>> {
        console_log!("Simulating database export - SQLite not yet available in workers-rs");
        
        // Return a mock SQLite file header
        let mock_header = b"SQLite format 3\x00".to_vec();
        Ok(mock_header)
    }
    
    async fn get_statistics(&self) -> Result<serde_json::Value> {
        console_log!("Simulating SQL: SELECT COUNT(*), COUNT(DISTINCT user_id), MIN(timestamp), MAX(timestamp) FROM messages");
        
        let total_messages = self.messages.len();
        let unique_users: std::collections::HashSet<_> = self.messages.values()
            .map(|m| &m.user_id)
            .collect();
        
        let timestamps: Vec<_> = self.messages.values()
            .map(|m| m.timestamp)
            .collect();
        
        let stats = serde_json::json!({
            "total_messages": total_messages,
            "unique_users": unique_users.len(),
            "first_message_time": timestamps.iter().min().copied(),
            "last_message_time": timestamps.iter().max().copied()
        });
        
        Ok(stats)
    }
}

#[durable_object]
impl DurableObject for ExampleSqliteDO {
    fn new(state: State, env: Env) -> Self {
        Self {
            state,
            env,
            messages: HashMap::new(),
            next_id: 1,
        }
    }
    
    async fn fetch(&mut self, mut req: Request) -> Result<Response> {
        self.init_database().await?;
        
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
                
                let id = message.id.unwrap_or(0).to_string();
                self.messages.insert(id.clone(), message.clone());
                self.next_id += 1;
                
                console_log!("Message stored with id: {}, total messages: {}", id, self.messages.len());
                
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
    let stub = namespace.id_from_name("ExampleSqliteDO")?.get_stub()?;
    stub.fetch_with_request(req).await
}