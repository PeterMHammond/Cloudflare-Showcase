use askama::Template;
use worker::*;
use serde_json::json;
use crate::utils::turnstile::validate_turnstile_token;

#[derive(Template)]
#[template(path = "turnstile.html")]
struct TurnstileTemplate {
    title: String,
    page_title: String,
    current_year: String,
    version: String,
    site_key: String,
}

pub async fn handler(mut req: Request, ctx: RouteContext<()>) -> Result<Response> {
    // Check if this is a POST request for token validation
    if req.method() == Method::Post {
        let secret_key = ctx.env.secret("TURNSTILE_SECRET_KEY")?.to_string();
        let form_data = req.form_data().await?;
        
        let token = match form_data.get("cf-turnstile-response") {
            Some(FormEntry::Field(value)) => value,
            _ => return Response::error("Missing Turnstile token", 400),
        };

        // Get the user's IP address (optional)
        let user_ip = req.headers().get("CF-Connecting-IP")?;

        // Validate the token and get raw response
        let mut verify_response = validate_turnstile_token(&token, &secret_key, user_ip.as_deref()).await?;

        // Add additional diagnostic information to the response
        if let serde_json::Value::Object(ref mut map) = verify_response {
            map.insert("user_ip".into(), json!(user_ip));
            map.insert("token_length".into(), json!(token.len()));
            map.insert("token_preview".into(), json!(format!("{}...", &token[..20])));
        }

        return Response::from_json(&verify_response);
    }

    // For GET requests, render the template
    let template = TurnstileTemplate {
        title: "Turnstile Test - Cloudflare Showcase".to_string(),
        page_title: "Turnstile Test".to_string(),
        current_year: "2024".to_string(),
        version: option_env!("CARGO_PKG_VERSION").unwrap_or_default().to_string(),
        site_key: "0x4AAAAAAA4VlVJxGznPxC-L".to_string(),
    };

    match template.render() {
        Ok(html) => Response::from_html(html),
        Err(err) => Response::error(format!("Failed to render template: {}", err), 500),
    }
} 