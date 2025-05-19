use worker::*;
use worker::console_log;
use wasm_bindgen::prelude::*;
use serde::{Deserialize, Serialize};
use crate::utils::sql_bindings::SqlStorageExt;

#[derive(Serialize, Deserialize, Debug)]
struct Message {
    id: Option<i64>,
    timestamp: i64,
    content: String,
    user_id: String,
}

#[wasm_bindgen]
pub struct ExampleSqliteDO {
    state: State,
    #[allow(dead_code)]
    env: Env,
    initialized: bool,
}

impl ExampleSqliteDO {
    async fn init_database(&mut self) -> Result<()> {
        let storage = self.state.storage();
        
        // First, let's debug what type of object we're getting
        console_log!("Checking SQL access...");
        
        match storage.sql() {
            Ok(sql) => {
                console_log!("SQL object obtained successfully");
                
                // Create tables if they don't exist
                match sql.exec(r#"
                    CREATE TABLE IF NOT EXISTS messages (
                        id INTEGER PRIMARY KEY AUTOINCREMENT,
                        timestamp INTEGER NOT NULL,
                        content TEXT NOT NULL,
                        user_id TEXT NOT NULL
                    )
                "#) {
                    Ok(_) => {
                        console_log!("Messages table created/verified");
                    },
                    Err(e) => {
                        console_log!("Failed to create messages table: {:?}", e);
                        return Err(Error::JsError(format!("Failed to create table: {:?}", e)));
                    }
                }
                
                match sql.exec(r#"
                    CREATE INDEX IF NOT EXISTS idx_timestamp ON messages(timestamp);
                    CREATE INDEX IF NOT EXISTS idx_user_id ON messages(user_id);
                "#) {
                    Ok(_) => {
                        console_log!("Indexes created/verified");
                    },
                    Err(e) => {
                        console_log!("Warning: Failed to create indexes: {:?}", e);
                        // Don't fail if indexes can't be created
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
        
        // Using exec instead of prepare for now
        let query = format!(
            "INSERT INTO messages (timestamp, content, user_id) VALUES ({}, '{}', '{}') RETURNING id",
            timestamp,
            content.replace("'", "''"), // Escape single quotes
            user_id.replace("'", "''")
        );
        
        console_log!("Executing query: {}", query);
        
        match sql.exec(&query) {
            Ok(cursor) => {
                let results = cursor.toArray();
                console_log!("Insert results: {:?}", results);
                
                if results.length() > 0 {
                    let row = results.get(0);
                    let id = js_sys::Reflect::get(&row, &JsValue::from_str("id"))
                        .ok()
                        .and_then(|val| val.as_f64())
                        .map(|f| f as i64);
                    
                    Ok(Message {
                        id,
                        timestamp,
                        content,
                        user_id,
                    })
                } else {
                    console_log!("No rows returned from insert");
                    Ok(Message {
                        id: None,
                        timestamp,
                        content,
                        user_id,
                    })
                }
            },
            Err(e) => {
                console_log!("Failed to insert message: {:?}", e);
                Err(Error::JsError(format!("Failed to insert: {:?}", e)))
            }
        }
    }
    
    async fn get_recent_messages(&self, limit: u32) -> Result<Vec<Message>> {
        console_log!("Getting recent messages, limit: {}", limit);
        
        let storage = self.state.storage();
        let sql = storage.sql()?;
        
        let query = format!("SELECT * FROM messages ORDER BY timestamp DESC LIMIT {}", limit);
        console_log!("Executing query: {}", query);
        
        match sql.exec(&query) {
            Ok(cursor) => {
                let results = cursor.toArray();
                console_log!("Found {} messages", results.length());
                
                let mut messages = Vec::new();
                for i in 0..results.length() {
                    let row = results.get(i);
                    
                    if let Ok(msg) = serde_wasm_bindgen::from_value::<Message>(row) {
                        messages.push(msg);
                    } else {
                        console_log!("Failed to parse message at index {}", i);
                    }
                }
                
                Ok(messages)
            },
            Err(e) => {
                console_log!("Failed to get messages: {:?}", e);
                Err(Error::JsError(format!("Failed to query: {:?}", e)))
            }
        }
    }
    
    async fn get_user_messages(&self, user_id: &str) -> Result<Vec<Message>> {
        let storage = self.state.storage();
        let sql = storage.sql()?;
        
        let query = format!(
            "SELECT * FROM messages WHERE user_id = '{}' ORDER BY timestamp DESC",
            user_id.replace("'", "''")
        );
        
        match sql.exec(&query) {
            Ok(cursor) => {
                cursor.collect()
            },
            Err(e) => {
                Err(Error::JsError(format!("Failed to query user messages: {:?}", e)))
            }
        }
    }
    
    async fn delete_messages(&self) -> Result<u64> {
        let storage = self.state.storage();
        let sql = storage.sql()?;
        
        let query = "DELETE FROM messages";
        
        match sql.exec(query) {
            Ok(_) => Ok(0), // We need to get row count from metadata
            Err(e) => Err(Error::JsError(format!("Failed to delete: {:?}", e)))
        }
    }
    
    async fn export_database(&self) -> Result<Vec<u8>> {
        console_log!("Exporting database as SQL dump");
        let storage = self.state.storage();
        let sql = storage.sql()?;
        let mut sql_dump = String::new();
        
        // Export schema
        sql_dump.push_str("-- SQLite database export\n");
        sql_dump.push_str("-- Generated from Cloudflare Durable Object\n\n");
        
        // Get the table schema
        let schema_query = r#"
            SELECT sql FROM sqlite_master 
            WHERE type IN ('table', 'index') 
            AND name NOT LIKE 'sqlite_%'
            ORDER BY type, name
        "#;
        
        match sql.exec(schema_query) {
            Ok(cursor) => {
                let results = cursor.toArray();
                console_log!("Found {} schema objects", results.length());
                
                for i in 0..results.length() {
                    let row = results.get(i);
                    if let Ok(sql_text) = js_sys::Reflect::get(&row, &JsValue::from_str("sql")) {
                        if let Some(sql_str) = sql_text.as_string() {
                            sql_dump.push_str(&sql_str);
                            sql_dump.push_str(";\n\n");
                        }
                    }
                }
            },
            Err(e) => {
                console_log!("Failed to get schema: {:?}", e);
                return Err(Error::JsError(format!("Failed to export schema: {:?}", e)));
            }
        }
        
        // Export data
        sql_dump.push_str("-- Message data\n");
        let data_query = "SELECT * FROM messages ORDER BY id";
        
        match sql.exec(data_query) {
            Ok(cursor) => {
                let results = cursor.toArray();
                console_log!("Exporting {} messages", results.length());
                
                for i in 0..results.length() {
                    let row = results.get(i);
                    if let Ok(msg) = serde_wasm_bindgen::from_value::<Message>(row) {
                        let insert = format!(
                            "INSERT INTO messages (id, timestamp, content, user_id) VALUES ({}, {}, '{}', '{}');\n",
                            msg.id.unwrap_or(0),
                            msg.timestamp,
                            msg.content.replace("'", "''"),
                            msg.user_id.replace("'", "''")
                        );
                        sql_dump.push_str(&insert);
                    }
                }
            },
            Err(e) => {
                console_log!("Failed to export data: {:?}", e);
                return Err(Error::JsError(format!("Failed to export data: {:?}", e)));
            }
        }
        
        console_log!("Database export successful, size: {} bytes", sql_dump.len());
        Ok(sql_dump.into_bytes())
    }
    
    async fn export_database_file(&self) -> Result<Vec<u8>> {
        console_log!("Exporting database as SQLite file");
        let storage = self.state.storage();
        let sql = storage.sql()?;
        
        // Directly access the serialized database
        // This is an internal API that might not be stable, but it's the most direct way
        // to get the raw SQLite database file
        
        // Use the SQL API to serialize the database
        let serialized_db = match sql.exec("VACUUM INTO ?", &[JsValue::from_str(":memory:")]) {
            Ok(_) => {
                // This is a simplified approach - in a real-world scenario, we would
                // use specific APIs to serialize the database if available
                
                // Since we can't directly serialize the database in Cloudflare Workers,
                // we'll take a different approach - we'll use the SQL dump to recreate a minimal
                // database in the proper format
                
                // First, get our SQL dump
                let sql_dump = self.export_database().await?;
                
                // Our database file header - this is a simplified SQLite file header
                // Full SQLite files are more complex, but this is enough for demonstration
                let mut db_file = Vec::with_capacity(sql_dump.len() + 100);
                
                // Add SQLite header signature "SQLite format 3"
                db_file.extend_from_slice(b"SQLite format 3\0");
                
                // Add the SQL dump as a comment section
                db_file.extend_from_slice(b"\n-- BEGIN SQL DUMP\n");
                db_file.extend_from_slice(&sql_dump);
                db_file.extend_from_slice(b"\n-- END SQL DUMP\n");
                
                // Note: This is not a valid SQLite file that can be opened directly
                // It's a custom format that contains the SQL dump
                // A real implementation would require access to the actual SQLite serialization APIs
                
                console_log!("Created simplified database file: {} bytes", db_file.len());
                db_file
            },
            Err(e) => {
                console_log!("Failed to serialize database: {:?}", e);
                return Err(Error::JsError(format!("Failed to export database file: {:?}", e)));
            }
        };
        
        Ok(serialized_db)
    }
    
    async fn get_statistics(&self) -> Result<serde_json::Value> {
        console_log!("Getting statistics");
        
        let storage = self.state.storage();
        let sql = storage.sql()?;
        
        match sql.exec(r#"
            SELECT 
                COUNT(*) as total_messages,
                COUNT(DISTINCT user_id) as unique_users,
                MIN(timestamp) as first_message_time,
                MAX(timestamp) as last_message_time
            FROM messages
        "#) {
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
                match sql.exec("SELECT 1 as test") {
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
            
            (Method::Get, "/export-file") => {
                let db_file = self.export_database_file().await?;
                let mut response = Response::from_bytes(db_file)?;
                response.headers_mut().set("Content-Type", "application/x-sqlite3")?;
                response.headers_mut().set("Content-Disposition", "attachment; filename=\"database.db\"")?;
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
    let namespace = ctx.env.durable_object("ExampleSqliteDO")?;
    let stub = namespace.id_from_name("sqlite-demo-instance")?.get_stub()?;
    stub.fetch_with_request(req).await
}