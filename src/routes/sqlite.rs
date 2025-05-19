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
    
    let namespace = ctx.env.durable_object("ExampleSqliteDO")?;
    // Use a consistent ID for the demo to maintain state across requests
    let stub = namespace.id_from_name("sqlite-demo-instance")?.get_stub()?;
    
    console_log!("Forwarding request to DO");
    let mut response = stub.fetch_with_request(req).await?;
    let status = response.status_code();
    
    console_log!("DO response status: {}", status);
    if status == 404 {
        let body: String = response.text().await.unwrap_or_default();
        console_log!("404 response body: {}", body);
    }
    
    Ok(response)
}