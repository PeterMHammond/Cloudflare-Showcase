# Analytics Engine Rust Bindings Specification

This document outlines the design principles and implementation details for creating Cloudflare Analytics Engine bindings within Rust-based Cloudflare Workers. It serves as a reference for implementing similar patterns in other projects.

## 1. Architecture Overview

The Analytics Engine Rust bindings implementation consists of the following key components:

1. **Rust Bindings to JavaScript Analytics Engine Interface**: A set of Rust bindings to the JavaScript Analytics Engine interface provided by Cloudflare Workers.
2. **Type Definitions**: TypeScript and Rust type definitions for Analytics Engine data structures.
3. **Extension Trait**: A trait to extend Worker functionality with Analytics Engine methods.
4. **Data Writing API**: Methods for writing data points to the Analytics Engine.
5. **Query API**: Methods for querying data from the Analytics Engine using SQL.
6. **Wrangler Configuration**: Configuration settings in `wrangler.toml` to enable Analytics Engine bindings.

## 2. Analytics Engine Interface Bindings

### 2.1 TypeScript Interface Definitions

Define TypeScript interfaces that represent the JavaScript Analytics Engine objects:

```typescript
// Type definitions for Analytics Engine in Durable Objects
interface AnalyticsEngineDataset {
  writeDataPoint(data: {
    blobs?: string[];
    doubles?: number[];
    indexes?: string[];
  }): void;
}

interface AnalyticsEngine {
  writeDataPoint(data: {
    blobs?: string[];
    doubles?: number[];
    indexes?: string[];
  }): void;
}
```

### 2.2 Rust Bindings to JavaScript

Create Rust bindings to the JavaScript Analytics Engine interface using `wasm_bindgen`:

```rust
use worker::*;
use wasm_bindgen::prelude::*;
use serde::{Deserialize, Serialize};

// Analytics Engine Dataset binding
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(typescript_type = "AnalyticsEngineDataset")]
    pub type AnalyticsEngineDataset;

    #[wasm_bindgen(method, js_name = writeDataPoint, catch)]
    pub fn write_data_point(
        this: &AnalyticsEngineDataset, 
        data: JsValue
    ) -> std::result::Result<(), JsValue>;
}

// Data point structure for writeDataPoint
#[derive(Serialize, Deserialize)]
pub struct DataPoint {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blobs: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doubles: Option<Vec<f64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub indexes: Option<Vec<String>>,
}

impl DataPoint {
    pub fn new() -> Self {
        Self {
            blobs: None,
            doubles: None,
            indexes: None,
        }
    }

    pub fn with_blobs(mut self, blobs: Vec<String>) -> Self {
        self.blobs = Some(blobs);
        self
    }

    pub fn with_doubles(mut self, doubles: Vec<f64>) -> Self {
        self.doubles = Some(doubles);
        self
    }

    pub fn with_indexes(mut self, indexes: Vec<String>) -> Self {
        self.indexes = Some(indexes);
        self
    }
}
```

### 2.3 Extension Trait for Analytics Engine Dataset

Create an extension trait for the Analytics Engine Dataset:

```rust
pub trait AnalyticsEngineExt {
    fn write_data_point(&self, data_point: DataPoint) -> Result<()>;
}

impl AnalyticsEngineExt for AnalyticsEngineDataset {
    fn write_data_point(&self, data_point: DataPoint) -> Result<()> {
        let js_value = serde_wasm_bindgen::to_value(&data_point)
            .map_err(|e| Error::JsError(format!("Failed to serialize data point: {:?}", e)))?;
        
        self.write_data_point(js_value)
            .map_err(|e| Error::JsError(format!("Failed to write data point: {:?}", e)))
    }
}
```

## 3. Analytics Engine Dataset Implementation

### 3.1 Basic Usage Pattern

Implement a simple pattern for writing data to Analytics Engine:

```rust
// Example function for recording page views
pub async fn record_page_view(
    analytics: &AnalyticsEngineDataset,
    url: &str,
    user_agent: &str,
    country: &str,
    load_time_ms: f64,
) -> Result<()> {
    let data_point = DataPoint::new()
        .with_blobs(vec![url.to_string(), user_agent.to_string(), country.to_string()])
        .with_doubles(vec![load_time_ms])
        .with_indexes(vec![uuid::Uuid::new_v4().to_string()]);
    
    analytics.write_data_point(data_point)
}
```

### 3.2 Error Handling

Add robust error handling for Analytics Engine operations:

```rust
pub fn write_metric(
    analytics: &AnalyticsEngineDataset,
    metric_name: &str,
    value: f64,
    labels: &[(&str, &str)],
) -> Result<()> {
    // Convert labels to blobs
    let mut blobs = vec![metric_name.to_string()];
    for (key, value) in labels {
        blobs.push(format!("{}:{}", key, value));
    }
    
    // Create a unique identifier
    let index = uuid::Uuid::new_v4().to_string();
    
    // Create and write the data point
    let data_point = DataPoint::new()
        .with_blobs(blobs)
        .with_doubles(vec![value])
        .with_indexes(vec![index]);
    
    match analytics.write_data_point(data_point) {
        Ok(()) => Ok(()),
        Err(e) => {
            console_error!("Failed to write metric {}: {:?}", metric_name, e);
            Err(e)
        }
    }
}
```

## 4. SQL API Integration

### 4.1 Query API Implementation

Implement a Rust wrapper for the Analytics Engine SQL API:

```rust
// SQL API Client
pub struct AnalyticsEngineSql {
    account_id: String,
    api_token: String,
}

impl AnalyticsEngineSql {
    pub fn new(account_id: String, api_token: String) -> Self {
        Self {
            account_id,
            api_token,
        }
    }
    
    pub async fn query(&self, sql: &str) -> Result<JsValue> {
        let url = format!(
            "https://api.cloudflare.com/client/v4/accounts/{}/analytics_engine/sql",
            self.account_id
        );
        
        let mut headers = Headers::new();
        headers.set("Authorization", &format!("Bearer {}", self.api_token))?;
        headers.set("Content-Type", "application/json")?;
        
        let request_init = RequestInit::new()
            .with_method(Method::Post)
            .with_headers(headers)
            .with_body(Some(JsValue::from_str(sql)));
        
        let request = Request::new_with_init(&url, &request_init)?;
        let response = Fetch::Request(request).send().await?;
        
        if !response.ok() {
            let status = response.status();
            let text = response.text().await?;
            return Err(Error::JsError(format!("SQL API Error ({}): {}", status, text)));
        }
        
        let result = response.json().await?;
        Ok(result)
    }
}
```

### 4.2 Query Utility Functions

Add utility functions for common query patterns:

```rust
impl AnalyticsEngineSql {
    // Get the total count of a metric
    pub async fn count_metric(&self, dataset: &str, metric_name: &str, days: u32) -> Result<u64> {
        let sql = format!(
            "SELECT COUNT(*) as count FROM {dataset} 
             WHERE blob1 = '{metric_name}' 
             AND timestamp > NOW() - INTERVAL '{days}' DAY",
            dataset = dataset,
            metric_name = metric_name,
            days = days
        );
        
        let result = self.query(&sql).await?;
        
        // Extract count from result
        if let Some(count) = js_sys::Reflect::get(&result, &JsValue::from_str("count"))
            .ok()
            .and_then(|v| v.as_f64())
            .map(|f| f as u64) {
            Ok(count)
        } else {
            Err(Error::JsError("Failed to extract count from query result".to_string()))
        }
    }
    
    // Get the average value of a metric
    pub async fn avg_metric(&self, dataset: &str, metric_name: &str, days: u32) -> Result<f64> {
        let sql = format!(
            "SELECT SUM(_sample_interval * double1) / SUM(_sample_interval) as avg_value 
             FROM {dataset} 
             WHERE blob1 = '{metric_name}' 
             AND timestamp > NOW() - INTERVAL '{days}' DAY",
            dataset = dataset,
            metric_name = metric_name,
            days = days
        );
        
        let result = self.query(&sql).await?;
        
        // Extract average from result
        if let Some(avg) = js_sys::Reflect::get(&result, &JsValue::from_str("avg_value"))
            .ok()
            .and_then(|v| v.as_f64()) {
            Ok(avg)
        } else {
            Err(Error::JsError("Failed to extract average from query result".to_string()))
        }
    }
}
```

## 5. Worker Integration

### 5.1 Environment Binding

Update the environment binding in the worker:

```rust
// Define the Env struct with Analytics Engine dataset binding
#[derive(Debug)]
pub struct Env {
    // Other bindings...
    pub analytics: AnalyticsEngineDataset,
}

// Implement the worker fetch handler with Analytics Engine usage
#[event(fetch)]
async fn fetch(req: Request, env: Env, ctx: Context) -> Result<Response> {
    console_error_panic_hook::set_once();
    
    // Record request metrics
    let url = req.url()?;
    let path = url.path();
    let user_agent = req.headers().get("User-Agent")?.unwrap_or_default();
    
    // Record a page view
    let data_point = DataPoint::new()
        .with_blobs(vec![
            "page_view".to_string(),
            path.to_string(),
            user_agent.to_string(),
        ])
        .with_doubles(vec![1.0]) // Count
        .with_indexes(vec![uuid::Uuid::new_v4().to_string()]);
    
    match env.analytics.write_data_point(data_point) {
        Ok(()) => console_log!("Recorded page view for {}", path),
        Err(e) => console_error!("Failed to record page view: {:?}", e),
    }
    
    // Continue handling the request...
    // ...
}
```

### 5.2 Middleware Integration

Create a middleware for automatic request tracking:

```rust
pub struct AnalyticsMiddleware;

impl AnalyticsMiddleware {
    pub async fn track_request(req: &Request, env: &Env, start_time: f64) -> Result<()> {
        let end_time = Date::now().as_millis() as f64;
        let duration_ms = end_time - start_time;
        
        let url = req.url()?;
        let path = url.path();
        let method = req.method().to_string();
        
        let data_point = DataPoint::new()
            .with_blobs(vec![
                "request".to_string(),
                method,
                path.to_string(),
            ])
            .with_doubles(vec![duration_ms])
            .with_indexes(vec![uuid::Uuid::new_v4().to_string()]);
        
        env.analytics.write_data_point(data_point)
    }
}
```

## 6. Wrangler Configuration

Configure Wrangler for Analytics Engine:

```toml
name = "your-project"
main = "build/worker/shim.mjs"
compatibility_date = "2023-12-01"

[build]
command = "cargo install -q worker-build && worker-build --release"

[[analytics_engine_datasets]]
binding = "analytics" 
dataset = "your_dataset_name"
```

## 7. Best Practices

1. **Data Point Structure**: Maintain consistent structure for data points to ensure easy querying.
2. **Blobs Organization**: Use the first blob element as a metric name or event type for easier filtering.
3. **Error Handling**: Implement robust error handling for Analytics Engine operations to ensure data reliability.
4. **Sampling Awareness**: Be aware of sampling for high-cardinality data and use `_sample_interval` in queries for accurate results.
5. **Batching**: Consider batching multiple metrics into fewer `writeDataPoint` calls to optimize performance.
6. **Unique Indexes**: Use UUIDs or other unique identifiers for indexes to ensure proper data distribution.
7. **Payload Size**: Keep blob sizes under limits (total size should not exceed 5120 bytes).
8. **Schema Design**: Design a consistent schema for your analytics data to simplify querying.
9. **Logging**: Include error logging for failed analytics writes to help with debugging.
10. **Middleware Approach**: Use a middleware approach to automatically track metrics for all requests.

## 8. Limitations and Considerations

1. **Write Limits**: Maximum of 25 data points per Worker invocation (per client HTTP request).
2. **Storage Duration**: Data is stored for 6 months by default. Plan accordingly for long-term analytics.
3. **Index Constraints**: Currently only supports a single index per data point (although the API accepts an array).
4. **Blob and Double Limits**: Up to 20 blobs and 20 doubles per data point.
5. **Size Limits**: Total size of all blobs in a request must not exceed 5120 bytes. Each index must not be more than 96 bytes.
6. **Local Development**: `writeDataPoint` might not be available in local development environments.
7. **Costs**: Be aware of the costs associated with Analytics Engine usage based on volume of data points.
8. **Latency Considerations**: Analytics writes should be non-blocking and not impact request handling performance.

## 9. Implementation Checklist

1. [ ] Define TypeScript interface definitions
2. [ ] Create Rust bindings to JavaScript Analytics Engine interface
3. [ ] Implement extension trait for Analytics Engine
4. [ ] Create data point structure and builder methods
5. [ ] Implement SQL API client
6. [ ] Add utility functions for common query patterns
7. [ ] Update worker environment bindings
8. [ ] Configure wrangler.toml for Analytics Engine
9. [ ] Add middleware for automatic request tracking
10. [ ] Implement error handling and logging

## 10. Usage Examples

### 10.1 Basic Metric Tracking

```rust
// Track a simple counter metric
pub async fn track_counter(env: &Env, name: &str, value: f64, tags: &[(&str, &str)]) -> Result<()> {
    let mut blobs = vec!["counter".to_string(), name.to_string()];
    
    // Add tags as blob entries
    for (key, value) in tags {
        blobs.push(format!("{}:{}", key, value));
    }
    
    let data_point = DataPoint::new()
        .with_blobs(blobs)
        .with_doubles(vec![value])
        .with_indexes(vec![uuid::Uuid::new_v4().to_string()]);
    
    env.analytics.write_data_point(data_point)
}
```

### 10.2 Performance Timing

```rust
// Track function execution time
pub async fn track_performance<F, Fut, T>(
    env: &Env,
    function_name: &str,
    f: F,
) -> Result<T>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<T>>,
{
    let start_time = Date::now().as_millis() as f64;
    let result = f().await;
    let end_time = Date::now().as_millis() as f64;
    let duration_ms = end_time - start_time;
    
    let data_point = DataPoint::new()
        .with_blobs(vec!["function_timing".to_string(), function_name.to_string()])
        .with_doubles(vec![duration_ms])
        .with_indexes(vec![uuid::Uuid::new_v4().to_string()]);
    
    match env.analytics.write_data_point(data_point) {
        Ok(()) => console_log!("Recorded timing for {}: {}ms", function_name, duration_ms),
        Err(e) => console_error!("Failed to record timing: {:?}", e),
    }
    
    result
}
```

### 10.3 Error Tracking

```rust
// Track errors
pub async fn track_error(env: &Env, error_type: &str, message: &str, path: &str) -> Result<()> {
    let data_point = DataPoint::new()
        .with_blobs(vec![
            "error".to_string(),
            error_type.to_string(),
            message.to_string(),
            path.to_string(),
        ])
        .with_doubles(vec![1.0]) // Count
        .with_indexes(vec![uuid::Uuid::new_v4().to_string()]);
    
    env.analytics.write_data_point(data_point)
}
```

## 11. References

- [Cloudflare Analytics Engine Documentation](https://developers.cloudflare.com/analytics/analytics-engine/)
- [Get Started with Analytics Engine](https://developers.cloudflare.com/analytics/analytics-engine/get-started/)
- [Analytics Engine SQL API](https://developers.cloudflare.com/analytics/analytics-engine/sql-api/)
- [Analytics Engine SQL Reference](https://developers.cloudflare.com/analytics/analytics-engine/sql-reference/)
- [Cloudflare Workers TypeScript Documentation](https://developers.cloudflare.com/workers/languages/typescript/)
- [Cloudflare Workers Rust SDK](https://github.com/cloudflare/workers-rs)
- [WebAssembly Bindings in Rust](https://rustwasm.github.io/wasm-bindgen/)