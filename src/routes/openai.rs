use askama::Template;
use worker::*;
use serde_json::{json, Value};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct ClientSecret {
    expires_at: i64,
    value: String,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct TurnDetection {
    create_response: bool,
    prefix_padding_ms: i32,
    silence_duration_ms: i32,
    threshold: f32,
    #[serde(rename = "type")]
    detection_type: String,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct OpenAISessionResponse {
    client_secret: ClientSecret,
    expires_at: i64,
    id: String,
    input_audio_format: String,
    instructions: String,
    max_response_output_tokens: String,
    modalities: Vec<String>,
    model: String,
    object: String,
    output_audio_format: String,
    temperature: f32,
    tool_choice: String,
    tools: Vec<Value>,
    turn_detection: TurnDetection,
    voice: String,
}

#[derive(Template)]
#[template(path = "openai.html")]
struct OpenAITemplate {
    title: String,
    page_title: String,
    current_year: String,
    version: String,
    token: String,
    expiry: String,
}

pub async fn handler(_req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let api_key = ctx.env.secret("OPENAI_API_KEY")?.to_string();
    
    let client = reqwest::Client::new();
    let response = client
        .post("https://api.openai.com/v1/realtime/sessions")
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&json!({
            "model": "gpt-4o-realtime-preview-2024-12-17",
            "instructions": "You are a just to respond with the word 'OpenAI is awesome!'.",
            "voice": "verse"
        }))
        .send()
        .await
        .map_err(|e| Error::from(e.to_string()))?;

    if !response.status().is_success() {
        let error_text = response.text().await.map_err(|e| Error::from(e.to_string()))?;
        console_log!("OpenAI API Error Response: {}", error_text);
        return Response::error(error_text, 500);
    }

    let session: OpenAISessionResponse = response.json().await.map_err(|e| Error::from(e.to_string()))?;
    console_log!("OpenAI API Response: {:?}", session);

    let template = OpenAITemplate {
        title: "OpenAI - Cloudflare Showcase".to_string(),
        page_title: "OpenAI".to_string(),
        current_year: "2024".to_string(),
        version: option_env!("CARGO_PKG_VERSION").unwrap_or_default().to_string(),
        token: session.client_secret.value,
        expiry: session.client_secret.expires_at.to_string(),
    };

    match template.render() {
        Ok(html) => Response::from_html(html),
        Err(err) => Response::error(format!("Failed to render template: {}", err), 500),
    }
} 