use askama::Template;
use worker::*;
use serde_json::json;
use crate::{utils::turnstile::validate_turnstile_token, BaseTemplate};

#[derive(Template)]
#[template(path = "turnstile.html")]
struct TurnstileTemplate {
    #[template(name = "base")]
    base: BaseTemplate,
}

pub async fn get_handler(_req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let base = BaseTemplate::new(&ctx, "Turnstile Test - Cloudflare Showcase", "Turnstile Validation").await?;
    
    let template = TurnstileTemplate { base };

    match template.render() {
        Ok(html) => Response::from_html(html),
        Err(err) => Response::error(format!("Failed to render template: {}", err), 500),
    }
}

pub async fn post_handler(mut req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let secret_key = ctx.env.secret("TURNSTILE_SECRET_KEY")?.to_string();
    let form_data = req.form_data().await?;
    
    let token = match form_data.get("cf-turnstile-response") {
        Some(FormEntry::Field(value)) => value,
        _ => return Response::error("Missing Turnstile token", 400),
    };

    let user_ip = req.headers().get("CF-Connecting-IP")?;
    let mut verify_response = validate_turnstile_token(&token, &secret_key, user_ip.as_deref()).await?;

    if let serde_json::Value::Object(ref mut map) = verify_response {
        map.insert("user_ip".into(), json!(user_ip));
        map.insert("token_length".into(), json!(token.len()));
        map.insert("token_preview".into(), json!(format!("{}...", &token[..20])));
    }

    Response::from_json(&verify_response)
} 