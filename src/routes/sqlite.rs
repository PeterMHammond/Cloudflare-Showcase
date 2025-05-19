use worker::*;
use crate::BaseTemplate;
use crate::utils::middleware::ValidationState;
use crate::utils::templates::render_template;
use serde_json::json;

pub async fn handler(_req: Request, ctx: RouteContext<ValidationState>) -> Result<Response> {
    let base = BaseTemplate::new(&ctx, "SQLite in Durable Objects - Cloudflare Showcase", "SQLite Demo").await?;
    
    let context = json!({
        "base": base
    });
    
    match render_template("sqlite.html", context) {
        Ok(html) => Response::from_html(html),
        Err(err) => {
            console_error!("Template render error: {}", err);
            Response::error(format!("Failed to render template: {}", err), 500)
        }
    }
}

pub async fn api_handler(req: Request, ctx: RouteContext<ValidationState>) -> Result<Response> {
    console_log!("SQLite API handler called: {} {}", req.method(), req.url()?.path());
    
    let namespace = ctx.env.durable_object("SqliteDO")?;
    // Use a consistent ID for the demo to maintain state across requests
    let stub = namespace.id_from_name("sqlite-demo-instance")?.get_stub()?;
    
    console_log!("Forwarding request to DO");
    match stub.fetch_with_request(req).await {
        Ok(response) => {
            let status = response.status_code();
            console_log!("DO response status: {}", status);
            Ok(response)
        },
        Err(e) => {
            console_log!("Error forwarding to DO: {:?}", e);
            Response::from_json(&json!({
                "error": format!("Internal server error: {}", e)
            })).map(|mut r| {
                r.headers_mut().set("Content-Type", "application/json").unwrap();
                r.with_status(500)
            })
        }
    }
}