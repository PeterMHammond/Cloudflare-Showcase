use worker::*;
use crate::utils::middleware::ValidationState;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

// Response structure for analytics API
#[derive(Serialize)]
struct ApiResponse {
    success: bool,
    message: String,
    data: Option<Value>,
}

// Query parameters for analytics API
#[derive(Deserialize)]
struct AnalyticsQuery {
    period: Option<String>,
    metric: Option<String>,
}

pub async fn metrics_handler(req: Request, ctx: RouteContext<ValidationState>) -> Result<Response> {
    // Extract query parameters
    let url = req.url()?;
    let query_str = url.query().unwrap_or("");
    
    // Manually parse query parameters
    let mut period = "7d".to_string();
    let mut metric = "all".to_string();
    
    for param in query_str.split('&') {
        if let Some((key, value)) = param.split_once('=') {
            match key {
                "period" => period = value.to_string(),
                "metric" => metric = value.to_string(),
                _ => {}
            }
        }
    }
    
    // Query Analytics Engine
    let analytics_data = query_analytics_data(&ctx.env, &period, &metric).await?;
    
    // Return the analytics data
    Ok(Response::from_json(&ApiResponse {
        success: true,
        message: "Analytics data retrieved successfully".to_string(),
        data: Some(analytics_data),
    })?)
}

async fn query_analytics_data(env: &Env, period: &str, metric: &str) -> Result<Value> {
    // Get the number of days to look back
    let days = match period {
        "24h" => 1,
        "7d" => 7,
        "30d" => 30,
        _ => 7, // Default to 7 days
    };
    
    // In a production environment, you'd query the Cloudflare Analytics Engine SQL API here
    // For now, we'll use simulated data but structure it as if it came from Analytics Engine
    
    // Check if ANALYTICS binding exists
    let has_analytics = env.var("ANALYTICS").is_ok();
    
    // Build simulated data for demo purposes
    // In production, replace this with actual Analytics Engine queries
    let pages = if has_analytics {
        // Real implementation would use Analytics Engine SQL API
        console_log!("Would query Analytics Engine for period: {} days, metric: {}", days, metric);
        
        // Simulate some analytics data that would normally come from Analytics Engine
        get_simulated_data(days)
    } else {
        // If Analytics Engine binding is not available, return minimal simulated data
        json!([
            {
                "url": "/",
                "title": "Home",
                "views": 42,
                "loadTime": 250,
                "scrollDepth": 65,
                "timeOnPage": 95,
                "bounceRate": 30
            }
        ])
    };
    
    // Calculate summary metrics
    let total_pages = pages.as_array().map_or(0, |arr| arr.len());
    let empty_vec = Vec::new();
    let page_data = pages.as_array().unwrap_or(&empty_vec);
    
    let total_views = page_data.iter()
        .filter_map(|p| p.get("views").and_then(|v| v.as_f64()))
        .sum::<f64>();
        
    let avg_load_time = page_data.iter()
        .filter_map(|p| p.get("loadTime").and_then(|v| v.as_f64()))
        .sum::<f64>() / total_pages as f64;
        
    let avg_scroll_depth = page_data.iter()
        .filter_map(|p| p.get("scrollDepth").and_then(|v| v.as_f64()))
        .sum::<f64>() / total_pages as f64;
        
    let avg_time_on_page = page_data.iter()
        .filter_map(|p| p.get("timeOnPage").and_then(|v| v.as_f64()))
        .sum::<f64>() / total_pages as f64;
    
    // Build the response with both summary and page-level data
    let result = json!({
        "summary": {
            "totalPages": total_pages,
            "totalViews": total_views,
            "avgLoadTime": avg_load_time,
            "avgScrollDepth": avg_scroll_depth,
            "avgTimeOnPage": avg_time_on_page,
            "period": period
        },
        "pages": pages
    });
    
    Ok(result)
}

// This function simulates data that would normally come from Analytics Engine
// In a production environment, this would be replaced with actual Analytics Engine queries
fn get_simulated_data(days: u32) -> Value {
    // Seed the random number generator with the number of days
    // This ensures consistent results for the same time period
    let seed = days as f64;
    
    // Helper function to generate a random number within a range
    let random = |min: f64, max: f64| -> f64 {
        min + ((seed * 9973.0).sin().abs() * (max - min))
    };
    
    // Generate data for each page
    let pages = vec![
        json!({
            "url": "/",
            "title": "Home",
            "views": (random(800.0, 1500.0) * days as f64 / 7.0) as u32,
            "loadTime": random(300.0, 400.0) as u32,
            "scrollDepth": random(60.0, 75.0) as u32,
            "timeOnPage": random(100.0, 150.0) as u32,
            "bounceRate": random(25.0, 35.0) as u32
        }),
        json!({
            "url": "/about",
            "title": "About Us",
            "views": (random(500.0, 800.0) * days as f64 / 7.0) as u32,
            "loadTime": random(250.0, 350.0) as u32,
            "scrollDepth": random(70.0, 85.0) as u32,
            "timeOnPage": random(180.0, 240.0) as u32,
            "bounceRate": random(20.0, 30.0) as u32
        }),
        json!({
            "url": "/analytics",
            "title": "Analytics Demo",
            "views": (random(300.0, 600.0) * days as f64 / 7.0) as u32,
            "loadTime": random(350.0, 450.0) as u32,
            "scrollDepth": random(75.0, 90.0) as u32,
            "timeOnPage": random(300.0, 400.0) as u32,
            "bounceRate": random(15.0, 25.0) as u32
        }),
        json!({
            "url": "/turnstile",
            "title": "Turnstile Demo",
            "views": (random(300.0, 500.0) * days as f64 / 7.0) as u32,
            "loadTime": random(280.0, 350.0) as u32,
            "scrollDepth": random(65.0, 80.0) as u32,
            "timeOnPage": random(120.0, 170.0) as u32,
            "bounceRate": random(18.0, 28.0) as u32
        }),
        json!({
            "url": "/websocket",
            "title": "WebSocket Demo",
            "views": (random(400.0, 600.0) * days as f64 / 7.0) as u32,
            "loadTime": random(320.0, 380.0) as u32,
            "scrollDepth": random(60.0, 75.0) as u32,
            "timeOnPage": random(160.0, 220.0) as u32,
            "bounceRate": random(25.0, 32.0) as u32
        }),
        json!({
            "url": "/sqlite",
            "title": "SQLite Demo",
            "views": (random(200.0, 350.0) * days as f64 / 7.0) as u32,
            "loadTime": random(300.0, 380.0) as u32,
            "scrollDepth": random(55.0, 70.0) as u32,
            "timeOnPage": random(140.0, 180.0) as u32,
            "bounceRate": random(30.0, 40.0) as u32
        }),
        json!({
            "url": "/study",
            "title": "Study Demo",
            "views": (random(150.0, 250.0) * days as f64 / 7.0) as u32,
            "loadTime": random(270.0, 330.0) as u32,
            "scrollDepth": random(75.0, 85.0) as u32,
            "timeOnPage": random(200.0, 250.0) as u32,
            "bounceRate": random(15.0, 25.0) as u32
        }),
        json!({
            "url": "/stt",
            "title": "Speech-to-Text Demo",
            "views": (random(250.0, 400.0) * days as f64 / 7.0) as u32,
            "loadTime": random(350.0, 420.0) as u32,
            "scrollDepth": random(70.0, 85.0) as u32,
            "timeOnPage": random(160.0, 220.0) as u32,
            "bounceRate": random(20.0, 30.0) as u32
        })
    ];
    
    json!(pages)
}