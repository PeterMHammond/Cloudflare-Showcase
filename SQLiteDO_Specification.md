# SQLite Durable Objects Specification

This document outlines the design principles and implementation details for creating SQLite bindings within Cloudflare Workers Durable Objects. It serves as a reference for implementing similar patterns in other projects.

## 1. Architecture Overview

The SQLite Durable Objects implementation consists of the following key components:

1. **Rust Bindings to JavaScript SQLite Interface**: A set of Rust bindings to the JavaScript SQLite interface provided by Cloudflare's Durable Objects.
2. **Durable Object Implementation**: A Rust struct implementing the Durable Object trait with SQLite functionality.
3. **SQL Files Organization**: Separation of SQL queries into standalone files for maintainability.
4. **API and Route Handlers**: HTTP API handlers for interacting with the SQLite database.
5. **Wrangler Configuration**: Configuration settings in `wrangler.toml` to enable SQLite in Durable Objects.

## 2. SQLite Interface Bindings

### 2.1 TypeScript Interface Definitions

Define TypeScript interfaces that represent the JavaScript SQLite objects:

```typescript
// Type definitions for SQLite in Durable Objects
interface DurableObjectStorage {
  sql: SqlStorage;
}

interface SqlStorage {
  exec(query: string, ...bindings: any[]): SqlStorageCursor;
  prepare(query: string): SqlPreparedStatement;
  dump(): ArrayBuffer;
  txn?: SqlTransaction;
}

interface SqlStorageCursor {
  readonly rowsRead: number;
  readonly rowsWritten: number;
  readonly durationMs: number;
  
  next(): any | undefined;
  toArray(): any[];
  forEach(callback: (row: any, index: number) => void): void;
}

interface SqlPreparedStatement {
  bind(...params: any[]): SqlStorageCursor;
  first(...params: any[]): any | null;
  all(...params: any[]): SqlResults;
  run(...params: any[]): SqlMeta;
}

interface SqlResults {
  readonly results: any[];
  readonly success: boolean;
  readonly meta: SqlMeta;
}

interface SqlMeta {
  readonly duration: number;
  readonly size_after?: number;
  readonly size_before?: number;
  readonly rows_written: number;
  readonly rows_read: number;
}

interface SqlTransaction {
  rollback(): void;
}
```

### 2.2 Rust Bindings to JavaScript

Create Rust bindings to the JavaScript SQLite interface using `wasm_bindgen`:

```rust
// SqlStorage interface that provides access to SQL methods
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(typescript_type = "SqlStorage")]
    pub type SqlStorage;

    // The only SQL method available in Durable Objects
    #[wasm_bindgen(method, catch)]
    pub fn exec(this: &SqlStorage, query: &str) -> std::result::Result<SqlStorageCursor, JsValue>;

    #[wasm_bindgen(method, catch)]
    pub fn dump(this: &SqlStorage) -> std::result::Result<Vec<u8>, JsValue>;
}

// SqlStorageCursor interface
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(typescript_type = "SqlStorageCursor")]
    pub type SqlStorageCursor;

    #[wasm_bindgen(method, js_name = columnNames)]
    pub fn column_names(this: &SqlStorageCursor) -> Array;

    #[wasm_bindgen(method, js_name = columnTypes)]
    pub fn column_types(this: &SqlStorageCursor) -> Array;

    #[wasm_bindgen(method, js_name = toArray)]
    pub fn toArray(this: &SqlStorageCursor) -> Array;

    #[wasm_bindgen(method)]
    pub fn one(this: &SqlStorageCursor) -> JsValue;

    #[wasm_bindgen(method)]
    pub fn all(this: &SqlStorageCursor) -> JsValue;
}
```

### 2.3 Extension Trait for Storage

Create an extension trait to access the SQLite functionality from Worker Storage:

```rust
// Extension trait for Storage to access SQL
pub trait SqlStorageExt {
    fn sql(&self) -> Result<SqlStorage>;
}

// Direct binding to DurableObjectStorage which has the sql property
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(extends = ::js_sys::Object, typescript_type = "DurableObjectStorage")]
    pub type DurableObjectStorage;
    
    #[wasm_bindgen(method, getter, catch)]
    pub fn sql(this: &DurableObjectStorage) -> std::result::Result<SqlStorage, JsValue>;
}

// Implement the extension for worker::Storage
impl SqlStorageExt for worker::Storage {
    fn sql(&self) -> Result<SqlStorage> {
        // Convert Storage to JsValue using wasm_bindgen's interop
        let storage_js: &JsValue = unsafe {
            // Safety: Storage is a wasm_bindgen type that wraps a JS object
            &*(self as *const worker::Storage as *const JsValue)
        };
        
        // Access the sql property directly using JS reflection
        let sql_js = js_sys::Reflect::get(storage_js, &JsValue::from_str("sql"))
            .map_err(|e| Error::JsError(format!("Failed to access sql property: {:?}", e)))?;
        
        if sql_js.is_null() || sql_js.is_undefined() {
            Err(Error::RustError("SQL storage not available. Make sure this Durable Object uses new_sqlite_classes in wrangler.toml".to_string()))
        } else {
            Ok(sql_js.unchecked_into::<SqlStorage>())
        }
    }
}
```

### 2.4 Cursor Result Collection

Add helper methods to collect results from a cursor into Rust types:

```rust
impl SqlStorageCursor {
    pub fn collect<T: for<'de> Deserialize<'de>>(&self) -> Result<Vec<T>> {
        let array = self.toArray();
        let len = array.length();
        let mut results = Vec::with_capacity(len as usize);
        
        for i in 0..len {
            let value = array.get(i);
            let deserialized: T = serde_wasm_bindgen::from_value(value)
                .map_err(|e| Error::JsError(format!("Failed to deserialize value: {:?}", e)))?;
            results.push(deserialized);
        }
        
        Ok(results)
    }
}
```

### 2.5 SQL Helper Functions

Implement helper functions for safe SQL query building:

```rust
// Helper functions for safe SQL query building
pub fn escape_sql_string(s: &str) -> String {
    s.replace("'", "''")
}

pub fn format_sql_value(value: &str) -> String {
    format!("'{}'", escape_sql_string(value))
}

pub fn format_sql_number<T: std::fmt::Display>(value: T) -> String {
    value.to_string()
}
```

## 3. Durable Object Implementation

### 3.1 Core Structure

Define the Durable Object struct with SQLite functionality:

```rust
#[wasm_bindgen]
pub struct SqliteDO {
    state: State,
    env: Env,
    initialized: bool,
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
    
    async fn fetch(&mut self, req: Request) -> Result<Response> {
        if !self.initialized {
            self.init_database().await?;
        }
        
        // Route handling logic
        // ...
    }
}
```

### 3.2 Database Initialization

Initialize the database schema when the Durable Object is first accessed:

```rust
async fn init_database(&mut self) -> Result<()> {
    let storage = self.state.storage();
    
    match storage.sql() {
        Ok(sql) => {
            // Create tables if they don't exist
            match sql.exec(include_str!("../sql/create_tables.sql")) {
                Ok(_) => {
                    console_log!("Tables created/verified");
                },
                Err(e) => {
                    return Err(Error::JsError(format!("Failed to create table: {:?}", e)));
                }
            }
            
            match sql.exec(include_str!("../sql/create_indexes.sql")) {
                Ok(_) => {
                    console_log!("Indexes created/verified");
                },
                Err(e) => {
                    console_log!("Warning: Failed to create indexes: {:?}", e);
                    // Don't fail if indexes can't be created
                }
            }
            
            self.initialized = true;
            Ok(())
        },
        Err(e) => {
            Err(e)
        }
    }
}
```

### 3.3 Data Access Methods

Implement methods for CRUD operations:

```rust
// Insert data
async fn add_message(&self, content: String, user_id: String) -> Result<Message> {
    let storage = self.state.storage();
    let sql = storage.sql()?;
    let timestamp = Date::now().as_millis() as i64;
    
    // Using exec with parameterized query
    let query = format!(
        "INSERT INTO messages (timestamp, content, user_id) VALUES ({}, '{}', '{}') RETURNING id",
        timestamp,
        content.replace("'", "''"), // Escape single quotes
        user_id.replace("'", "''")
    );
    
    match sql.exec(&query) {
        Ok(cursor) => {
            let results = cursor.toArray();
            
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
                Ok(Message {
                    id: None,
                    timestamp,
                    content,
                    user_id,
                })
            }
        },
        Err(e) => {
            Err(Error::JsError(format!("Failed to insert: {:?}", e)))
        }
    }
}

// Query data
async fn get_recent_messages(&self, limit: u32) -> Result<Vec<Message>> {
    let storage = self.state.storage();
    let sql = storage.sql()?;
    
    let query = include_str!("../sql/get_recent_messages.sql").replace("{}", &limit.to_string());
    
    match sql.exec(&query) {
        Ok(cursor) => {
            cursor.collect() // Use the collect extension method
        },
        Err(e) => {
            Err(Error::JsError(format!("Failed to query: {:?}", e)))
        }
    }
}

// Delete data
async fn delete_messages(&self) -> Result<u64> {
    let storage = self.state.storage();
    let sql = storage.sql()?;
    
    let query = include_str!("../sql/delete_messages.sql");
    
    match sql.exec(query) {
        Ok(_) => Ok(0), // We need to get row count from metadata
        Err(e) => Err(Error::JsError(format!("Failed to delete: {:?}", e)))
    }
}
```

### 3.4 API Request Routing

Handle HTTP requests with a router pattern:

```rust
async fn fetch(&mut self, mut req: Request) -> Result<Response> {
    if !self.initialized {
        self.init_database().await?;
    }
    
    let url = req.url()?;
    let path = url.path();
    
    // Strip the /api prefix from the path
    let api_path = path.strip_prefix("/api").unwrap_or(path);
    
    match (req.method(), api_path) {
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
        
        // Additional routes...
        
        _ => {
            Response::error("Not Found", 404)
        }
    }
}
```

## 4. SQL Files Organization

Organize SQL queries in separate files for better maintainability:

### 4.1 Directory Structure

```
src/
  sql/
    create_tables.sql
    create_indexes.sql
    get_recent_messages.sql
    get_user_messages.sql
    delete_messages.sql
    get_statistics.sql
    get_schema.sql
    get_all_messages.sql
    simple_test.sql
```

### 4.2 Sample SQL Files

**create_tables.sql**:
```sql
CREATE TABLE IF NOT EXISTS messages (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp INTEGER NOT NULL,
    content TEXT NOT NULL,
    user_id TEXT NOT NULL
)
```

**create_indexes.sql**:
```sql
CREATE INDEX IF NOT EXISTS idx_timestamp ON messages(timestamp);
CREATE INDEX IF NOT EXISTS idx_user_id ON messages(user_id);
```

**get_recent_messages.sql**:
```sql
SELECT * FROM messages ORDER BY timestamp DESC LIMIT {}
```

## 5. Route Handler Integration

Implement route handlers to interface with the Durable Object:

```rust
pub async fn handler(_req: Request, ctx: RouteContext<ValidationState>) -> Result<Response> {
    let base = BaseTemplate::new(&ctx, "SQLite Demo", "SQLite in Durable Objects - Cloudflare Showcase").await?;
    
    let context = json!({
        "base": base
    });
    
    match render_template("sqlite.html", context) {
        Ok(html) => Response::from_html(html),
        Err(err) => {
            Response::error(format!("Failed to render template: {}", err), 500)
        }
    }
}

pub async fn api_handler(req: Request, ctx: RouteContext<ValidationState>) -> Result<Response> {
    let namespace = ctx.env.durable_object("SqliteDO")?;
    // Use a consistent ID for the demo to maintain state across requests
    let stub = namespace.id_from_name("sqlite-demo-instance")?.get_stub()?;
    
    match stub.fetch_with_request(req).await {
        Ok(response) => Ok(response),
        Err(e) => {
            Response::from_json(&json!({
                "error": format!("Internal server error: {}", e)
            })).map(|mut r| {
                r.headers_mut().set("Content-Type", "application/json").unwrap();
                r.with_status(500)
            })
        }
    }
}
```

## 6. Wrangler Configuration

Configure Wrangler for SQLite Durable Objects:

```toml
name = "your-project"
main = "build/worker/shim.mjs"
compatibility_date = "2023-12-01"

[[durable_objects.bindings]]
name = "SqliteDO"
class_name = "SqliteDO"

[[migrations]]
tag = "v1"
new_sqlite_classes = ["SqliteDO"]
```

## 7. Best Practices

1. **Initialization**: Always initialize the database schema during the first request.
2. **SQL Injection Prevention**: Use parameterized queries or proper string escaping to prevent SQL injection.
3. **Error Handling**: Implement comprehensive error handling for SQL operations.
4. **Transactions**: Use transactions for multiple related operations to ensure data consistency.
5. **Query Organization**: Store SQL queries in separate files for better maintainability.
6. **Connection Reuse**: Reuse the SQL connection within a request to minimize overhead.
7. **Result Serialization**: Use `serde` for serializing and deserializing SQL results to Rust types.
8. **Indexes**: Create appropriate indexes to optimize query performance.
9. **Data Validation**: Validate input data before executing SQL queries.
10. **Logging**: Use console logging for debugging and monitoring SQL operations.

## 8. Limitations and Considerations

1. **Storage Limits**: SQLite in Durable Objects has storage limitations defined by Cloudflare.
2. **Query Complexity**: Avoid extremely complex queries that might timeout or consume excessive resources.
3. **Concurrency**: Be aware of concurrency limitations with SQLite in Durable Objects.
4. **Data Persistence**: Understand that data is persisted within the Durable Object's lifecycle.
5. **Migration Strategy**: Have a strategy for migrating data when schema changes are needed.
6. **Backup Strategy**: Implement export functionality for backing up important data.
7. **Cost Considerations**: Be aware of the additional costs associated with Durable Objects storage.
8. **Compatibility**: Check compatibility with the latest Cloudflare Workers features.

## 9. Implementation Checklist

1. [ ] Define TypeScript interface definitions
2. [ ] Create Rust bindings to JavaScript SQLite interface
3. [ ] Implement extension trait for storage
4. [ ] Setup SQL files organization
5. [ ] Implement Durable Object with SQLite functionality
6. [ ] Add database initialization logic
7. [ ] Implement CRUD operations
8. [ ] Add HTTP API request routing
9. [ ] Configure wrangler.toml for SQLite Durable Objects
10. [ ] Implement route handlers for web interface

## 10. References

- [Cloudflare Durable Objects Documentation](https://developers.cloudflare.com/workers/runtime-apis/durable-objects/)
- [Cloudflare Workers Rust SDK](https://github.com/cloudflare/workers-rs)
- [SQLite Documentation](https://www.sqlite.org/docs.html)
- [WebAssembly Bindings in Rust](https://rustwasm.github.io/wasm-bindgen/)