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

    let form_body = form_data
        .iter()
        .map(|(k, v)| format!("{}={}", k, js_sys::encode_uri_component(v).as_string().unwrap()))
        .collect::<Vec<_>>()
        .join("&");

    let mut headers = Headers::new();
    headers.set("Content-Type", "application/x-www-form-urlencoded")?;

    let request = Request::new_with_init(
        api_url,
        RequestInit::new()
            .with_method(Method::Post)
            .with_headers(headers)
            .with_body(Some(js_sys::Uint8Array::from(form_body.as_bytes()).into())),
    )?;

    let mut response = Fetch::Request(request).send().await
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