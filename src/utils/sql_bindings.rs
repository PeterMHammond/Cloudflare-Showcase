use worker::{Error, wasm_bindgen, js_sys};
use serde::de::DeserializeOwned;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use js_sys::Array;

type Result<T> = std::result::Result<T, Error>;

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

pub struct Cursor {
    inner: SqlStorageCursor,
}

impl Cursor {
    fn new(inner: SqlStorageCursor) -> Self {
        Self { inner }
    }
    
    /// Get all rows as a vector of the specified type
    pub fn collect<T: DeserializeOwned>(self) -> Result<Vec<T>> {
        let array = self.inner.toArray();
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
    
    /// Get the first row, if any
    pub fn first<T: DeserializeOwned>(self) -> Result<Option<T>> {
        let array = self.inner.toArray();
        if array.length() == 0 {
            return Ok(None);
        }
        
        let row = array.get(0);
        let value: T = serde_wasm_bindgen::from_value(row)
            .map_err(|e| Error::JsError(format!("Failed to deserialize row: {}", e)))?;
        Ok(Some(value))
    }
}

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

impl SqlStorage {
    /// Execute a SQL query and return a cursor over the results
    pub fn execute(&self, query: &str) -> Result<Cursor> {
        let cursor = self.exec(query)
            .map_err(|e| Error::JsError(format!("SQL execution failed: {:?}", e)))?;
        Ok(Cursor::new(cursor))
    }
    
    /// Prepare a SQL statement for execution with bound parameters (D1-style API)
    pub fn prepare(&self, query: &str) -> PreparedStatement {
        PreparedStatement::new(query.to_string(), self)
    }
    
    /// Dump the entire database as a binary blob
    pub fn dump_db(&self) -> Result<Vec<u8>> {
        self.dump()
            .map_err(|e| Error::JsError(format!("Failed to dump database: {:?}", e)))
    }
    
    /// Initialize the database with migrations
    pub fn migrate(&self, migrations: &[Migration]) -> Result<()> {
        // Create migrations table if it doesn't exist
        self.execute(
            "CREATE TABLE IF NOT EXISTS __migrations (
                version INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                applied_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )"
        )?;
        
        // Get current version
        let current_version = self
            .execute("SELECT COALESCE(MAX(version), 0) as version FROM __migrations")?
            .first::<serde_json::Value>()?
            .and_then(|v| v.get("version").and_then(|v| v.as_i64()))
            .unwrap_or(0) as i32;
        
        // Apply pending migrations
        for migration in migrations {
            if migration.version > current_version {
                // Execute migration
                self.execute(migration.sql)?;
                
                // Record migration - using direct SQL to avoid the prepare issue
                let query = format!(
                    "INSERT INTO __migrations (version, name) VALUES ({}, '{}')",
                    migration.version,
                    escape_sql_string(migration.name)
                );
                self.execute(&query)?;
            }
        }
        
        Ok(())
    }
    
    /// Execute multiple SQL statements in a transaction
    pub fn transaction<F, R>(&self, f: F) -> Result<R>
    where
        F: FnOnce(&SqlStorage) -> Result<R>,
    {
        self.execute("BEGIN")?;
        
        match f(self) {
            Ok(result) => {
                self.execute("COMMIT")?;
                Ok(result)
            }
            Err(e) => {
                let _ = self.execute("ROLLBACK");
                Err(e)
            }
        }
    }
}

/// Prepared SQL statement that mimics D1's prepare/bind API
pub struct PreparedStatement<'a> {
    query: String,
    sql: &'a SqlStorage,
    bindings: Vec<JsValue>,
}

impl<'a> PreparedStatement<'a> {
    fn new(query: String, sql: &'a SqlStorage) -> Self {
        Self {
            query,
            sql,
            bindings: Vec::new(),
        }
    }
    
    /// Bind parameters to the prepared statement (D1-style)
    /// Accepts an array of values to bind to ? placeholders
    pub fn bind<I, T>(mut self, params: I) -> Self 
    where
        I: IntoIterator<Item = T>,
        T: IntoJsValue,
    {
        self.bindings = params.into_iter().map(|v| v.into_js_value()).collect();
        self
    }
    
    /// Bind a single value (convenience method)
    pub fn bind_value<T: IntoJsValue>(mut self, value: T) -> Self {
        self.bindings.push(value.into_js_value());
        self
    }
    
    /// Execute the statement and return all rows
    pub fn all<T: DeserializeOwned>(&self) -> Result<Vec<T>> {
        let query = self.build_query()?;
        let cursor = self.sql.exec(&query)
            .map_err(|e| Error::JsError(format!("SQL execution failed: {:?}", e)))?;
        Cursor::new(cursor).collect()
    }
    
    /// Execute the statement and return the first row
    pub fn first<T: DeserializeOwned>(&self) -> Result<Option<T>> {
        let query = self.build_query()?;
        let cursor = self.sql.exec(&query)
            .map_err(|e| Error::JsError(format!("SQL execution failed: {:?}", e)))?;
        Cursor::new(cursor).first()
    }
    
    /// Execute the statement without returning rows (INSERT, UPDATE, DELETE)
    /// Returns the number of affected rows
    pub fn run(&self) -> Result<usize> {
        let query = self.build_query()?;
        let cursor = self.sql.exec(&query)
            .map_err(|e| Error::JsError(format!("SQL execution failed: {:?}", e)))?;
        
        // For DML statements, the cursor might contain metadata about affected rows
        // We'll try to extract this information
        let array = cursor.toArray();
        Ok(array.length() as usize)
    }
    
    /// Build the final query with parameters substituted
    fn build_query(&self) -> Result<String> {
        let mut query = self.query.clone();
        let mut param_index = 0;
        
        // Replace ? placeholders with actual values
        while let Some(pos) = query.find('?') {
            if param_index >= self.bindings.len() {
                return Err(Error::RustError("Not enough parameters bound to query".into()));
            }
            
            let value = &self.bindings[param_index];
            let replacement = Self::format_value(value)?;
            
            query = format!("{}{}{}", &query[..pos], replacement, &query[pos + 1..]);
            param_index += 1;
        }
        
        // Also handle numbered placeholders like ?1, ?2, etc.
        for (i, binding) in self.bindings.iter().enumerate() {
            let placeholder = format!("?{}", i + 1);
            if query.contains(&placeholder) {
                let replacement = Self::format_value(binding)?;
                query = query.replace(&placeholder, &replacement);
            }
        }
        
        if param_index < self.bindings.len() && !query.contains('?') {
            return Err(Error::RustError("Too many parameters bound to query".into()));
        }
        
        Ok(query)
    }
    
    /// Format a JavaScript value for SQL
    fn format_value(value: &JsValue) -> Result<String> {
        if value.is_null() {
            Ok("NULL".to_string())
        } else if value.is_undefined() {
            Ok("NULL".to_string())
        } else if let Some(b) = value.as_bool() {
            Ok(if b { "1".to_string() } else { "0".to_string() })
        } else if let Some(n) = value.as_f64() {
            Ok(n.to_string())
        } else if let Some(s) = value.as_string() {
            // Escape single quotes in strings
            Ok(format!("'{}'", s.replace('\'', "''")))
        } else {
            // Try to convert to string
            Ok(format!("'{}'", value.as_string().unwrap_or_default().replace('\'', "''")))
        }
    }
}

/// Simple migration system for SQLite
pub struct Migration {
    pub version: i32,
    pub name: &'static str,
    pub sql: &'static str,
}

/// Extension trait for converting Rust types to JsValue for binding
pub trait IntoJsValue {
    fn into_js_value(self) -> JsValue;
}

impl IntoJsValue for &str {
    fn into_js_value(self) -> JsValue {
        JsValue::from_str(self)
    }
}

impl IntoJsValue for String {
    fn into_js_value(self) -> JsValue {
        JsValue::from_str(&self)
    }
}

impl IntoJsValue for &String {
    fn into_js_value(self) -> JsValue {
        JsValue::from_str(self)
    }
}

impl IntoJsValue for i32 {
    fn into_js_value(self) -> JsValue {
        JsValue::from_f64(self as f64)
    }
}

impl IntoJsValue for i64 {
    fn into_js_value(self) -> JsValue {
        JsValue::from_f64(self as f64)
    }
}

impl IntoJsValue for f64 {
    fn into_js_value(self) -> JsValue {
        JsValue::from_f64(self)
    }
}

impl IntoJsValue for bool {
    fn into_js_value(self) -> JsValue {
        JsValue::from_bool(self)
    }
}

impl<T: IntoJsValue> IntoJsValue for Option<T> {
    fn into_js_value(self) -> JsValue {
        match self {
            Some(value) => value.into_js_value(),
            None => JsValue::NULL,
        }
    }
}