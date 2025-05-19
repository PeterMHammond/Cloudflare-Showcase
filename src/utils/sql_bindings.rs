use worker::{Error, wasm_bindgen, js_sys};
use serde::Deserialize;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use js_sys::{Array};

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