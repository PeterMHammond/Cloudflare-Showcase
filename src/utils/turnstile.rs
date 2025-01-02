use worker::*;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct TurnstileResponse {
    pub success: bool,
    #[serde(rename = "error-codes")]
    pub error_codes: Option<Vec<String>>,
}

pub async fn validate_turnstile_token(
    token: &str,
    secret_key: &str,
    user_ip: Option<&str>,
) -> Result<TurnstileResponse> {
    let api_url = "https://challenges.cloudflare.com/turnstile/v0/siteverify";

    let mut form_data = vec![
        ("secret", secret_key),
        ("response", token),
    ];

    if let Some(ip) = user_ip {
        form_data.push(("remoteip", ip));
    }

    let client = reqwest::Client::new();
    let response = client
        .post(api_url)
        .form(&form_data)
        .send()
        .await
        .map_err(|e| {
            console_error!("Failed to call Turnstile API: {}", e);
            Error::from(format!("Failed to call Turnstile API: {}", e))
        })?;

    response
        .json::<TurnstileResponse>()
        .await
        .map_err(|e| {
            console_error!("Invalid Turnstile response: {}", e);
            Error::from(format!("Invalid Turnstile response: {}", e))
        })
} 