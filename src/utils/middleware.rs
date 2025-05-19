use cookie::{Cookie, CookieJar, Key};
use worker::*;

pub struct ValidationState {
    pub is_validated: bool,
    pub validation_message: String,
}

impl Default for ValidationState {
    fn default() -> Self {
        Self {
            is_validated: false,
            validation_message: String::new(),
        }
    }
}

pub async fn validate_turnstile(req: Request, env: &worker::Env, _ctx: &Context) -> Result<(Request, ValidationState)> {
    let mut state = ValidationState::default();
    
    state.validation_message = match req.headers().get("Cookie")? {
        Some(header) => {
            let header_str = header.to_string();
            let secret_key = env.secret("TURNSTILE_SECRET_KEY")?.to_string();
            let secret_bytes = secret_key.as_bytes();
            let cookie_key = Key::derive_from(secret_bytes);    
            let mut jar = CookieJar::new();
            
            for cookie_str in header_str.split(';').map(|s| s.trim().to_owned()) {
                if let Ok(cookie) = Cookie::parse(cookie_str) {
                    jar.signed_mut(&cookie_key).add(cookie);
                }
            }
            
            match jar.signed(&cookie_key).get("turnstile_validated") {
                Some(cookie) => {
                    state.is_validated = true;
                    cookie.value().to_string()
                },
                None => "Invalid Turnstile".to_string(),
            }
        }
        None => "Cookie header not found".to_string(),
    };

    Ok((req, state))
} 