use worker::*;
use crate::utils::templates::render_template;
use crate::utils::middleware::ValidationState;
use serde_json::json;

pub async fn handler(_req: Request, _ctx: RouteContext<ValidationState>) -> Result<Response> {
    let context = json!({
        "title": "Version - Cloudflare Showcase",
        "page_title": "Version",
        "current_year": "2024",
        "version": option_env!("CARGO_PKG_VERSION").unwrap_or_default(),
    });

    match render_template("version.html", context) {
        Ok(html) => Response::from_html(html),
        Err(err) => Response::error(format!("Failed to render template: {}", err), 500),
    }
} 