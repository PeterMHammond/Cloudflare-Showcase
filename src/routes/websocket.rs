use worker::*;
use crate::BaseTemplate;
use crate::utils::middleware::ValidationState;
use crate::utils::templates::render_template;
use serde_json::json;

pub async fn handler(_req: Request, ctx: RouteContext<ValidationState>) -> Result<Response> {
    let base = BaseTemplate::new(&ctx, "WebSocket - Cloudflare Showcase", "WebSocket").await?;
    
    let context = json!({
        "base": base
    });

    match render_template("websocket.html", context) {
        Ok(html) => Response::from_html(html),
        Err(err) => Response::error(format!("Failed to render template: {}", err), 500),
    }
} 