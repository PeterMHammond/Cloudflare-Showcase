use askama::Template;
use cookie::{Cookie, CookieJar, Key};
use serde::{Deserialize, Serialize};
use worker::*;
use crate::utils::turnstile::validate_turnstile_token;

#[derive(Template)]
#[template(path = "verify.html")]
struct VerifyTemplate {
    site_key: String,
}

#[derive(Deserialize)]
struct VerifyRequest {
    token: String,
}

#[derive(Serialize)]
struct VerifyResponse {
    success: bool,
    redirect: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<Vec<String>>,
}

pub async fn get_handler(_req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let site_key = ctx.env.secret("TURNSTILE_SITE_KEY")?.to_string();
    let template = VerifyTemplate { site_key };

    match template.render() {
        Ok(html) => Response::from_html(html),
        Err(err) => Response::error(format!("Failed to render template: {}", err), 500),
    }
}

pub async fn post_handler(mut req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let secret_key = ctx.env.secret("TURNSTILE_SECRET_KEY")?.to_string();
    let user_ip = req.headers().get("CF-Connecting-IP")?;
    
    let verify_req: VerifyRequest = req.json().await?;
    let turnstile_response = validate_turnstile_token(&verify_req.token, &secret_key, user_ip.as_deref()).await?;
    
    if turnstile_response.success {
        let secret_bytes = secret_key.as_bytes();
        let cookie_key = Key::derive_from(secret_bytes);
        let mut jar = CookieJar::new();
        
        let cookie = Cookie::build(("turnstile_validated", "true"))
            .path("/")
            .max_age(cookie::time::Duration::days(7))
            .http_only(true)
            .secure(true)
            .same_site(cookie::SameSite::Strict)
            .build();
        jar.signed_mut(&cookie_key).add(cookie);
        
        let mut response = Response::from_json(&VerifyResponse {
            success: true,
            redirect: Some("/".to_string()),
            error: None,
        })?;

        for cookie in jar.delta() {
            response.headers_mut().append("Set-Cookie", &cookie.to_string())?;
        }
        
        Ok(response)
    } else {
        Response::from_json(&VerifyResponse {
            success: false,
            redirect: None,
            error: turnstile_response.error_codes,
        })
    }
} 