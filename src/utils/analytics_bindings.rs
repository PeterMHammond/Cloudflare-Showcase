use worker::*;
use wasm_bindgen::prelude::*;
use serde::{Deserialize, Serialize};

// Define the Analytics Engine Dataset type
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

// Extension trait for Analytics Engine Dataset
pub trait AnalyticsEngineExt {
    fn write_data_point(&self, data_point: &DataPoint) -> Result<()>;
}

impl AnalyticsEngineExt for AnalyticsEngineDataset {
    fn write_data_point(&self, data_point: &DataPoint) -> Result<()> {
        let js_value = serde_wasm_bindgen::to_value(data_point)
            .map_err(|e| Error::JsError(format!("Failed to serialize data point: {:?}", e)))?;
        
        self.write_data_point(js_value)
            .map_err(|e| Error::JsError(format!("Failed to write data point: {:?}", e)))
    }
}

// Utility functions for recording analytics
pub async fn record_page_view(
    analytics: &AnalyticsEngineDataset,
    url: &str,
    user_agent: &str,
    load_time_ms: f64,
) -> Result<()> {
    let data_point = DataPoint::new()
        .with_blobs(vec![
            "page_view".to_string(),
            url.to_string(),
            user_agent.to_string(),
        ])
        .with_doubles(vec![load_time_ms])
        .with_indexes(vec![uuid::Uuid::new_v4().to_string()]);
    
    Ok(analytics.write_data_point(&data_point)?)
}

pub async fn record_time_on_page(
    analytics: &AnalyticsEngineDataset,
    url: &str,
    session_id: &str,
    time_seconds: f64,
) -> Result<()> {
    let data_point = DataPoint::new()
        .with_blobs(vec![
            "time_on_page".to_string(),
            url.to_string(),
            session_id.to_string(),
        ])
        .with_doubles(vec![time_seconds])
        .with_indexes(vec![session_id.to_string()]);
    
    Ok(analytics.write_data_point(&data_point)?)
}

pub async fn record_user_interaction(
    analytics: &AnalyticsEngineDataset,
    url: &str,
    session_id: &str,
    interaction_type: &str,
) -> Result<()> {
    let data_point = DataPoint::new()
        .with_blobs(vec![
            "interaction".to_string(),
            url.to_string(),
            session_id.to_string(),
            interaction_type.to_string(),
        ])
        .with_doubles(vec![1.0])
        .with_indexes(vec![session_id.to_string()]);
    
    Ok(analytics.write_data_point(&data_point)?)
}

// SQL API Client for querying Analytics Engine data
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
    
    pub async fn query(&self, sql: &str) -> Result<serde_json::Value> {
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
        let mut response = Fetch::Request(request).send().await?;
        
        if response.status_code() != 200 {
            let status = response.status_code();
            let text = response.text().await?;
            return Err(Error::JsError(format!("SQL API Error ({}): {}", status, text)));
        }
        
        let result = response.json::<serde_json::Value>().await?;
        Ok(result)
    }
    
    // Get the average page load time
    pub async fn get_avg_page_load_time(&self, dataset: &str, days: u32) -> Result<f64> {
        let sql = format!(
            "SELECT SUM(_sample_interval * double1) / SUM(_sample_interval) as avg_load_time 
             FROM {dataset} 
             WHERE blob1 = 'page_view' 
             AND timestamp > NOW() - INTERVAL '{days}' DAY",
            dataset = dataset,
            days = days
        );
        
        let result = self.query(&sql).await?;
        
        // Extract average from result
        match result.get("avg_load_time").and_then(|v| v.as_f64()) {
            Some(avg) => Ok(avg),
            None => Err(Error::JsError("Failed to extract average from query result".to_string()))
        }
    }
    
    // Get the average time on page
    pub async fn get_avg_time_on_page(&self, dataset: &str, days: u32) -> Result<f64> {
        let sql = format!(
            "SELECT SUM(_sample_interval * double1) / SUM(_sample_interval) as avg_time_seconds 
             FROM {dataset} 
             WHERE blob1 = 'time_on_page' 
             AND timestamp > NOW() - INTERVAL '{days}' DAY",
            dataset = dataset,
            days = days
        );
        
        let result = self.query(&sql).await?;
        
        // Extract average from result
        match result.get("avg_time_seconds").and_then(|v| v.as_f64()) {
            Some(avg) => Ok(avg),
            None => Err(Error::JsError("Failed to extract average from query result".to_string()))
        }
    }
    
    // Get page view count
    pub async fn get_page_view_count(&self, dataset: &str, days: u32) -> Result<u64> {
        let sql = format!(
            "SELECT COUNT(*) as view_count 
             FROM {dataset} 
             WHERE blob1 = 'page_view' 
             AND timestamp > NOW() - INTERVAL '{days}' DAY",
            dataset = dataset,
            days = days
        );
        
        let result = self.query(&sql).await?;
        
        // Extract count from result
        match result.get("view_count").and_then(|v| v.as_f64()).map(|f| f as u64) {
            Some(count) => Ok(count),
            None => Err(Error::JsError("Failed to extract count from query result".to_string()))
        }
    }
}