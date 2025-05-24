use worker::*;
use worker::console_log;
use wasm_bindgen::prelude::*;
use serde::{Deserialize, Serialize};
use crate::utils::sql_bindings::{SqlStorageExt, Migration};

#[derive(Serialize, Deserialize, Debug)]
struct Message {
    id: Option<i64>,
    timestamp: i64,
    content: String,
    user_id: String,
}

#[wasm_bindgen]
pub struct SqliteDO {
    state: State,
    #[allow(dead_code)]
    env: Env,
    initialized: bool,
}

// Define migrations for the database
const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "create_messages_table",
        sql: include_str!("../sql/create_tables.sql"),
    },
    Migration {
        version: 2,
        name: "create_indexes",
        sql: include_str!("../sql/create_indexes.sql"),
    },
];

impl SqliteDO {
    async fn init_database(&mut self) -> Result<()> {
        let storage = self.state.storage();
        
        console_log!("Initializing database with migration system...");
        
        match storage.sql() {
            Ok(sql) => {
                console_log!("SQL object obtained successfully");
                
                // Apply migrations
                match sql.migrate(MIGRATIONS) {
                    Ok(()) => {
                        console_log!("All migrations applied successfully");
                    },
                    Err(e) => {
                        console_log!("Failed to apply migrations: {:?}", e);
                        return Err(e);
                    }
                }
                
                console_log!("Database initialized successfully");
                self.initialized = true;
                Ok(())
            },
            Err(e) => {
                console_log!("Failed to access SQL: {:?}", e);
                Err(e)
            }
        }
    }
    
    async fn add_message(&self, content: String, user_id: String) -> Result<Message> {
        console_log!("Adding message: {} from user: {}", content, user_id);
        
        let storage = self.state.storage();
        let sql = storage.sql()?;
        let timestamp = Date::now().as_millis() as i64;
        
        // Use the new PreparedStatement API for safer queries
        let id = sql.prepare("INSERT INTO messages (timestamp, content, user_id) VALUES (?, ?, ?) RETURNING id")
            .bind_value(timestamp)
            .bind_value(content.as_str())
            .bind_value(user_id.as_str())
            .first::<serde_json::Value>()?
            .and_then(|row| row.get("id").and_then(|v| v.as_i64()));
        
        console_log!("Message inserted with id: {:?}", id);
        
        Ok(Message {
            id,
            timestamp,
            content,
            user_id,
        })
    }
    
    async fn get_recent_messages(&self, limit: u32) -> Result<Vec<Message>> {
        console_log!("Getting recent messages, limit: {}", limit);
        
        let storage = self.state.storage();
        let sql = storage.sql()?;
        
        // Use PreparedStatement for parameterized query
        let messages = sql.prepare("SELECT * FROM messages ORDER BY timestamp DESC LIMIT ?")
            .bind_value(limit as i32)
            .all::<Message>()?;
        
        console_log!("Found {} messages", messages.len());
        Ok(messages)
    }
    
    async fn get_user_messages(&self, user_id: &str) -> Result<Vec<Message>> {
        let storage = self.state.storage();
        let sql = storage.sql()?;
        
        // Use PreparedStatement for safe parameter binding
        sql.prepare("SELECT * FROM messages WHERE user_id = ? ORDER BY timestamp DESC")
            .bind_value(user_id)
            .all::<Message>()
    }
    
    async fn delete_messages(&self) -> Result<u64> {
        let storage = self.state.storage();
        let sql = storage.sql()?;
        
        // Use execute method for non-parameterized queries
        sql.execute("DELETE FROM messages")?;
        Ok(0) // SQLite DO doesn't provide row count directly
    }
    
    async fn bulk_insert_messages(&self, messages: Vec<(String, String)>) -> Result<usize> {
        let storage = self.state.storage();
        let sql = storage.sql()?;
        
        // Demonstrate transaction usage
        sql.transaction(|sql| {
            let mut count = 0;
            let timestamp = Date::now().as_millis() as i64;
            
            for (content, user_id) in messages {
                sql.prepare("INSERT INTO messages (timestamp, content, user_id) VALUES (?, ?, ?)")
                    .bind_value(timestamp)
                    .bind_value(content.as_str())
                    .bind_value(user_id.as_str())
                    .run()?;
                count += 1;
            }
            
            Ok(count)
        })
    }
    
    async fn export_database(&self) -> Result<Vec<u8>> {
        console_log!("Exporting database as binary dump");
        let storage = self.state.storage();
        let sql = storage.sql()?;
        
        // Use the new dump_db method for a complete binary database dump
        sql.dump_db()
    }
    
    
    async fn get_statistics(&self) -> Result<serde_json::Value> {
        console_log!("Getting statistics");
        
        let storage = self.state.storage();
        let sql = storage.sql()?;
        
        match sql.exec(include_str!("../sql/get_statistics.sql")) {
            Ok(cursor) => {
                let stats = cursor.toArray();
                console_log!("Stats query returned {} rows", stats.length());
                
                if stats.length() > 0 {
                    let row = stats.get(0);
                    console_log!("Stats row: {:?}", row);
                    Ok(serde_wasm_bindgen::from_value(row)?)
                } else {
                    Ok(serde_json::json!({
                        "total_messages": 0,
                        "unique_users": 0,
                        "first_message_time": null,
                        "last_message_time": null
                    }))
                }
            },
            Err(e) => {
                console_log!("Failed to get statistics: {:?}", e);
                Err(Error::JsError(format!("Failed to query stats: {:?}", e)))
            }
        }
    }
    
    async fn sql_test(&self) -> Result<Response> {
        console_log!("Running SQL test");
        
        use crate::utils::sql_bindings::SqlStorageExt;
        
        let storage = self.state.storage();
        
        // Test 1: Can we access the sql property?
        match storage.sql() {
            Ok(sql) => {
                console_log!("Successfully accessed SQL object");
                
                // Try a simple query to verify it works
                match sql.exec(include_str!("../sql/simple_test.sql")) {
                    Ok(cursor) => {
                        let result = cursor.toArray();
                        if result.length() > 0 {
                            Response::from_json(&serde_json::json!({
                                "success": true,
                                "message": "SQL access successful",
                                "test_result": "Query returned results"
                            }))
                        } else {
                            Response::from_json(&serde_json::json!({
                                "success": false,
                                "message": "Query returned no results"
                            }))
                        }
                    },
                    Err(e) => Response::from_json(&serde_json::json!({
                        "success": false,
                        "message": format!("Failed to execute test query: {:?}", e)
                    }))
                }
            },
            Err(e) => {
                console_log!("Failed to access SQL: {:?}", e);
                Response::from_json(&serde_json::json!({
                    "success": false,
                    "message": format!("Failed to access SQL: {}", e)
                }))
            }
        }
    }
}

#[durable_object]
impl DurableObject for SqliteDO {
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
        }
        
        let url = req.url()?;
        let path = url.path();
        console_log!("SQLite DO received request: {} {}", req.method(), path);
        console_log!("Full URL: {}", url.to_string());
        
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
            
            (Method::Delete, "/messages") => {
                console_log!("Processing DELETE /messages request");
                console_log!("Deleting all messages");
                let deleted = self.delete_messages().await?;
                console_log!("Deleted {} messages", deleted);
                
                Response::from_json(&serde_json::json!({
                    "deleted": deleted,
                    "message": "All messages deleted successfully"
                }))
            }
            
            (Method::Delete, "/old") => {
                // Remove this endpoint - we should just use the /messages endpoint
                // For backward compatibility, we'll just redirect to /messages
                console_log!("DELETE /old route is deprecated, redirecting to /messages");
                let deleted = self.delete_messages().await?;
                console_log!("Deleted {} messages", deleted);
                
                Response::from_json(&serde_json::json!({
                    "deleted": deleted,
                    "message": "All messages deleted successfully"
                }))
            }
            
            (Method::Post, "/bulk-insert") => {
                #[derive(Deserialize)]
                struct BulkInsertRequest {
                    messages: Vec<MessageInput>,
                }
                
                #[derive(Deserialize)]
                struct MessageInput {
                    content: String,
                    user_id: String,
                }
                
                let body: BulkInsertRequest = req.json().await
                    .map_err(|e| Error::RustError(format!("Failed to parse JSON: {}", e)))?;
                
                let messages: Vec<(String, String)> = body.messages
                    .into_iter()
                    .map(|m| (m.content, m.user_id))
                    .collect();
                
                let count = self.bulk_insert_messages(messages).await?;
                
                Response::from_json(&serde_json::json!({
                    "success": true,
                    "inserted": count
                }))
            }
            
            (Method::Get, "/stats") => {
                let stats = self.get_statistics().await?;
                Response::from_json(&stats)
            }
            
            (Method::Get, "/export") => {
                let dump = self.export_database().await?;
                let mut response = Response::from_bytes(dump)?;
                response.headers_mut().set("Content-Type", "text/plain; charset=utf-8")?;
                response.headers_mut().set("Content-Disposition", "attachment; filename=\"database.sql\"")?;
                Ok(response)
            }
            
            
            (Method::Get, "/sql-test") => {
                console_log!("Handling SQL test request");
                self.sql_test().await
            }
            
            _ => {
                console_log!("No match found for: method={:?}, api_path='{}', full_path='{}']", req.method(), api_path, path);
                Response::error(format!("Not Found: {} {}", req.method(), api_path), 404)
            }
        }
    }
}

pub async fn handler(req: Request, ctx: RouteContext<crate::utils::middleware::ValidationState>) -> Result<Response> {
    let namespace = ctx.env.durable_object("SqliteDO")?;
    let stub = namespace.id_from_name("sqlite-demo-instance")?.get_stub()?;
    stub.fetch_with_request(req).await
}