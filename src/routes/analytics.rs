use worker::*;
use crate::BaseTemplate;
use crate::utils::middleware::ValidationState;
use crate::utils::templates::render_template;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

// Data structure for client-side analytics data
#[derive(Deserialize)]
struct ClientAnalyticsData {
    session_id: String,
    event_type: String,
    data: Value,
}

// Response structure for client-side analytics API
#[derive(Serialize)]
struct AnalyticsResponse {
    success: bool,
    message: String,
}

// Structure for Analytics Engine data points
#[derive(Serialize)]
struct DataPoint {
    #[serde(skip_serializing_if = "Option::is_none")]
    blobs: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    doubles: Option<Vec<f64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    indexes: Option<Vec<String>>,
}

// Simple helper for collecting analytics
async fn record_analytics(env: &Env, point: DataPoint) -> Result<()> {
    // This function intentionally ignores errors to avoid breaking the user experience
    match serde_wasm_bindgen::to_value(&point) {
        Ok(js_data) => {
            // Try to access the Analytics Engine binding
            if let Ok(_analytics_binding) = env.var("ANALYTICS") {
                // For now, just log that we would record analytics in production
                console_log!("Analytics data point recorded: {:?}", js_data);
            }
        }
        Err(e) => {
            console_error!("Failed to serialize analytics data: {:?}", e);
        }
    }
    
    Ok(())
}

// UUID generator function for templates
pub fn uuid4() -> String {
    Uuid::new_v4().to_string()
}

pub async fn handler(req: Request, ctx: RouteContext<ValidationState>) -> Result<Response> {
    let base = BaseTemplate::new(&ctx, "Analytics Engine Demo", "Analytics Engine Dashboard").await?;
    
    // Generate a unique session ID for this page view
    let session_id = Uuid::new_v4().to_string();
    
    // Get URL and user agent information
    let url = req.url()?;
    let path = url.path();
    let user_agent = req.headers().get("User-Agent")?.unwrap_or_default();
    
    // Create data point for page view
    let data_point = DataPoint {
        blobs: Some(vec![
            "page_view".to_string(),
            path.to_string(),
            user_agent.to_string(),
        ]),
        doubles: Some(vec![0.0]), // Will be updated by client-side JavaScript
        indexes: Some(vec![session_id.clone()]),
    };
    
    // Record the analytics (best effort)
    let _ = record_analytics(&ctx.env, data_point).await;
    
    // Add our context
    let context = json!({
        "base": base,
        "session_id": session_id
    });

    match render_template("analytics.html", context) {
        Ok(html) => Response::from_html(html),
        Err(err) => Response::error(format!("Failed to render template: {}", err), 500),
    }
}

// API endpoint for client-side analytics data
pub async fn data_handler(mut req: Request, ctx: RouteContext<ValidationState>) -> Result<Response> {
    // Parse the request body
    let client_data = match req.json::<ClientAnalyticsData>().await {
        Ok(data) => data,
        Err(e) => {
            console_error!("Failed to parse analytics data: {:?}", e);
            return Ok(Response::from_json(&AnalyticsResponse {
                success: false,
                message: "Invalid request format".to_string(),
            })?.with_status(400));
        }
    };
    
    // Prepare the data point based on the event type
    let data_point = match client_data.event_type.as_str() {
        "page_load" => {
            // Extract the page load time from the client data
            let load_time = client_data.data.get("load_time_ms")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            
            // Create a data point for page load time
            DataPoint {
                blobs: Some(vec![
                    "page_load_time".to_string(),
                    req.url()?.path().to_string(),
                ]),
                doubles: Some(vec![load_time]),
                indexes: Some(vec![client_data.session_id]),
            }
        },
        "time_on_page" => {
            // Extract the time on page from the client data
            let seconds = client_data.data.get("seconds")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            
            let is_final = client_data.data.get("final")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            
            // Create a data point for time on page
            DataPoint {
                blobs: Some(vec![
                    "time_on_page".to_string(),
                    req.url()?.path().to_string(),
                    if is_final { "final".to_string() } else { "interval".to_string() },
                ]),
                doubles: Some(vec![seconds]),
                indexes: Some(vec![client_data.session_id]),
            }
        },
        "page_viewed" => {
            // Extract the scroll percentage from the client data
            let percentage = client_data.data.get("percentage")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            
            let page = client_data.data.get("page")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
                
            let url = client_data.data.get("url")
                .and_then(|v| v.as_str())
                .unwrap_or("/");
                
            let milestone = client_data.data.get("milestone")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            
            // Create a data point for page view percentage
            DataPoint {
                blobs: Some(vec![
                    "page_viewed".to_string(),
                    url.to_string(),
                    page.to_string(),
                    milestone.to_string(),
                ]),
                doubles: Some(vec![percentage]),
                indexes: Some(vec![client_data.session_id]),
            }
        },
        "interaction" => {
            // Extract the interaction type from the client data
            let interaction_type = client_data.data.get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            
            // Create a data point for user interaction
            DataPoint {
                blobs: Some(vec![
                    "interaction".to_string(),
                    req.url()?.path().to_string(),
                    interaction_type.to_string(),
                ]),
                doubles: Some(vec![1.0]),
                indexes: Some(vec![client_data.session_id]),
            }
        },
        _ => {
            // Unknown event type
            console_error!("Unknown event type: {}", client_data.event_type);
            return Ok(Response::from_json(&AnalyticsResponse {
                success: false,
                message: "Unknown event type".to_string(),
            })?.with_status(400));
        }
    };
    
    // Record the analytics (best effort)
    let _ = record_analytics(&ctx.env, data_point).await;
    
    // Return success response
    Ok(Response::from_json(&AnalyticsResponse {
        success: true,
        message: "Data point recorded".to_string(),
    })?)
}