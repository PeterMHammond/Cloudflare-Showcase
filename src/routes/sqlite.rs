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
        Err(err) => Response::error(format!("Failed to render template: {}", err), 500),
    }
}

pub async fn api_handler(req: Request, ctx: RouteContext<ValidationState>) -> Result<Response> {
    let namespace = ctx.env.durable_object("ExampleSqliteDO")?;
    let stub = namespace.id_from_name("ExampleSqliteDO")?.get_stub()?;
    stub.fetch_with_request(req).await
}