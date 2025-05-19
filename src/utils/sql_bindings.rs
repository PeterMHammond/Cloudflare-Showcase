use worker::{Error, Storage, wasm_bindgen, js_sys};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;
use js_sys::{Array, Object, Reflect};

type Result<T> = std::result::Result<T, Error>;

// SqlStorage interface that provides access to SQL methods
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(typescript_type = "SqlStorage")]
    pub type SqlStorage;

    #[wasm_bindgen(method, catch)]
    pub fn exec(this: &SqlStorage, query: &str) -> std::result::Result<SqlStorageCursor, JsValue>;

    #[wasm_bindgen(method, catch, variadic)]
    pub fn exec_with_bindings(this: &SqlStorage, query: &str, bindings: &Array) -> std::result::Result<SqlStorageCursor, JsValue>;

    #[wasm_bindgen(method, catch)]
    pub fn prepare(this: &SqlStorage, query: &str) -> std::result::Result<SqlPreparedStatement, JsValue>;

    #[wasm_bindgen(method, catch)]
    pub fn dump(this: &SqlStorage) -> std::result::Result<Vec<u8>, JsValue>;

    #[wasm_bindgen(method, getter)]
    pub fn txn(this: &SqlStorage) -> Option<SqlTransaction>;
}

// SqlStorageCursor for iterating through query results
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(typescript_type = "SqlStorageCursor")]
    pub type SqlStorageCursor;

    #[wasm_bindgen(method, getter)]
    pub fn rowsRead(this: &SqlStorageCursor) -> u64;

    #[wasm_bindgen(method, getter)]
    pub fn rowsWritten(this: &SqlStorageCursor) -> u64;

    #[wasm_bindgen(method, getter)]
    pub fn durationMs(this: &SqlStorageCursor) -> f64;

    #[wasm_bindgen(method)]
    pub fn next(this: &SqlStorageCursor) -> JsValue; // Returns row or undefined

    #[wasm_bindgen(method)]
    pub fn toArray(this: &SqlStorageCursor) -> Array;

    #[wasm_bindgen(method)]
    pub fn forEach(this: &SqlStorageCursor, callback: &js_sys::Function);
}

// SqlPreparedStatement for parameterized queries
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(typescript_type = "SqlPreparedStatement")]
    pub type SqlPreparedStatement;

    #[wasm_bindgen(method, catch)]
    pub fn bind(this: &SqlPreparedStatement) -> std::result::Result<SqlStorageCursor, JsValue>;

    #[wasm_bindgen(method, catch, variadic)]
    pub fn bind_with_params(this: &SqlPreparedStatement, params: &Array) -> std::result::Result<SqlStorageCursor, JsValue>;

    #[wasm_bindgen(method, catch)]
    pub fn first(this: &SqlPreparedStatement) -> std::result::Result<Option<Object>, JsValue>;

    #[wasm_bindgen(method, catch, variadic)]
    pub fn first_with_params(this: &SqlPreparedStatement, params: &Array) -> std::result::Result<Option<Object>, JsValue>;

    #[wasm_bindgen(method, catch)]
    pub fn all(this: &SqlPreparedStatement) -> std::result::Result<SqlResults, JsValue>;

    #[wasm_bindgen(method, catch, variadic)]
    pub fn all_with_params(this: &SqlPreparedStatement, params: &Array) -> std::result::Result<SqlResults, JsValue>;

    #[wasm_bindgen(method, catch)]
    pub fn run(this: &SqlPreparedStatement) -> std::result::Result<SqlMeta, JsValue>;

    #[wasm_bindgen(method, catch, variadic)]
    pub fn run_with_params(this: &SqlPreparedStatement, params: &Array) -> std::result::Result<SqlMeta, JsValue>;
}

// SqlResults for batch results
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(typescript_type = "SqlResults")]
    pub type SqlResults;

    #[wasm_bindgen(method, getter)]
    pub fn results(this: &SqlResults) -> Array;

    #[wasm_bindgen(method, getter)]
    pub fn success(this: &SqlResults) -> bool;

    #[wasm_bindgen(method, getter)]
    pub fn meta(this: &SqlResults) -> SqlMeta;
}

// SqlMeta for query metadata
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(typescript_type = "SqlMeta")]
    pub type SqlMeta;

    #[wasm_bindgen(method, getter)]
    pub fn duration(this: &SqlMeta) -> f64;

    #[wasm_bindgen(method, getter)]
    pub fn size_after(this: &SqlMeta) -> Option<u64>;

    #[wasm_bindgen(method, getter)]
    pub fn size_before(this: &SqlMeta) -> Option<u64>;

    #[wasm_bindgen(method, getter)]
    pub fn rows_written(this: &SqlMeta) -> u64;

    #[wasm_bindgen(method, getter)]
    pub fn rows_read(this: &SqlMeta) -> u64;
}

// SqlTransaction for transactional operations
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(typescript_type = "SqlTransaction")]
    pub type SqlTransaction;

    #[wasm_bindgen(method, catch)]
    pub fn rollback(this: &SqlTransaction) -> std::result::Result<(), JsValue>;
}

// Extension traits to add SQL support to existing types
pub trait SqlStorageExt {
    fn sql(&self) -> Result<SqlStorage>;
}

impl SqlStorageExt for Storage {
    fn sql(&self) -> Result<SqlStorage> {
        // NOTE: This is a placeholder implementation
        // In production, the storage object should have a sql property
        // This would need to be properly exposed by workers-rs
        
        // For now, we'll return an error indicating SQLite is not available
        Err(Error::RustError("SQLite storage is not yet available in workers-rs. This demo showcases the API that would be available once integrated.".to_string()))
    }
}

// Helper functions for working with SQL results
impl SqlStorageCursor {
    pub fn collect<T: for<'de> Deserialize<'de>>(&self) -> Result<Vec<T>> {
        let array = self.toArray();
        let mut results = Vec::new();
        
        for i in 0..array.length() {
            let item = array.get(i);
            let parsed: T = serde_wasm_bindgen::from_value(item)
                .map_err(|e| Error::RustError(format!("Failed to deserialize row: {}", e)))?;
            results.push(parsed);
        }
        
        Ok(results)
    }
}

// Convenience methods for SqlPreparedStatement
impl SqlPreparedStatement {
    pub fn bind_values(&self, values: &[impl Serialize]) -> Result<SqlStorageCursor> {
        let array = Array::new();
        for value in values {
            let js_value = serde_wasm_bindgen::to_value(value)
                .map_err(|e| Error::RustError(format!("Failed to serialize parameter: {}", e)))?;
            array.push(&js_value);
        }
        self.bind_with_params(&array).map_err(|e| Error::JsError(format!("{:?}", e)))
    }

    pub fn first_value<T: for<'de> Deserialize<'de>>(&self, values: &[impl Serialize]) -> Result<Option<T>> {
        let array = Array::new();
        for value in values {
            let js_value = serde_wasm_bindgen::to_value(value)
                .map_err(|e| Error::RustError(format!("Failed to serialize parameter: {}", e)))?;
            array.push(&js_value);
        }
        
        match self.first_with_params(&array) {
            Ok(Some(obj)) => {
                let value = serde_wasm_bindgen::from_value(obj.into())
                    .map_err(|e| Error::RustError(format!("Failed to deserialize result: {}", e)))?;
                Ok(Some(value))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(Error::JsError(format!("{:?}", e)))
        }
    }
}

// Helper for executing SQL with bindings
impl SqlStorage {
    pub fn exec_many(&self, query: &str, values: &[impl Serialize]) -> Result<SqlStorageCursor> {
        let array = Array::new();
        for value in values {
            let js_value = serde_wasm_bindgen::to_value(value)
                .map_err(|e| Error::RustError(format!("Failed to serialize parameter: {}", e)))?;
            array.push(&js_value);
        }
        self.exec_with_bindings(query, &array).map_err(|e| Error::JsError(format!("{:?}", e)))
    }
}

// Macro for easy SQL query building
#[macro_export]
macro_rules! sql_query {
    ($storage:expr, $query:literal) => {
        $storage.sql()?.exec($query).map_err(|e| worker::Error::JsError(format!("{:?}", e)))
    };
    ($storage:expr, $query:literal, $($param:expr),*) => {
        $storage.sql()?.exec_many($query, &[$($param),*])
    };
}