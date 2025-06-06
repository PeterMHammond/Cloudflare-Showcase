use worker::*;
use serde_json::json;
use serde::{Deserialize, Serialize};
use crate::{utils::turnstile::validate_turnstile_token, BaseTemplate};
use crate::utils::middleware::ValidationState;
use crate::utils::templates::render_template;

#[derive(Deserialize)]
struct ValidateRequest {
    token: String,
}

#[derive(Serialize)]
struct ApiResponse {
    success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    debug_info: Option<serde_json::Value>,
}

pub async fn get_handler(_req: Request, ctx: RouteContext<ValidationState>) -> Result<Response> {
    let base = BaseTemplate::new(&ctx, "Turnstile Test - Cloudflare Showcase", "Turnstile Validation").await?;
    
    let context = json!({
        "base": base
    });

    match render_template("turnstile.html", context) {
        Ok(html) => Response::from_html(html),
        Err(err) => Response::error(format!("Failed to render template: {}", err), 500),
    }
}

pub async fn post_handler(mut req: Request, ctx: RouteContext<ValidationState>) -> Result<Response> {
    let secret_key = ctx.env.secret("TURNSTILE_SECRET_KEY")?.to_string();
    let user_ip = req.headers().get("CF-Connecting-IP")?;
    
    let validate_req: ValidateRequest = req.json().await.map_err(|err| {
        console_error!("Failed to parse JSON: {}", err);
        err
    })?;
    
    let turnstile_response = validate_turnstile_token(&validate_req.token, &secret_key, user_ip.as_deref()).await?;
    
    let debug_info = if cfg!(debug_assertions) {
        Some(json!({
            "user_ip": user_ip,
            "token_length": validate_req.token.len(),
            "token_preview": format!("{}...", &validate_req.token[..20])
        }))
    } else {
        None
    };

    let api_response = ApiResponse {
        success: turnstile_response.success,
        error: turnstile_response.error_codes,
        debug_info,
    };

    Response::from_json(&api_response)
} 