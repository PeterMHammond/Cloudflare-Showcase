use worker::*;
use serde_json::{json, Value};
use serde::{Deserialize, Serialize};
use crate::BaseTemplate;
use crate::utils::middleware::ValidationState;
use crate::utils::templates::render_template;

#[derive(Debug, Serialize, Deserialize)]
struct ClientSecret {
    expires_at: i64,
    value: String,
}

#[allow(dead_code)]
#[derive(Debug, Serialize, Deserialize)]
struct TurnDetection {
    create_response: bool,
    prefix_padding_ms: i32,
    silence_duration_ms: i32,
    threshold: f32,
    #[serde(rename = "type")]
    detection_type: String,
}

#[allow(dead_code)]
#[derive(Debug, Serialize, Deserialize)]
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

pub async fn handler(req: Request, ctx: RouteContext<ValidationState>) -> Result<Response> {
    let api_key = ctx.env.secret("OPENAI_API_KEY")?.to_string();    
    let headers = req.headers();
    console_log!("User IP: {:?}", headers);
    let headers = req.headers();

    if let Some(header_value) = headers.get("X-OpenAI-Client-Secret")? {
        console_log!("Header value: {}", header_value);
        return Response::ok(header_value);
    }
    console_log!("No header value found");

    let mut headers = Headers::new();
    headers.set("Authorization", &format!("Bearer {}", api_key))?;
    headers.set("Content-Type", "application/json")?;
    
    let request_body = json!({
        "model": "gpt-4o-realtime-preview-2024-12-17",
        "instructions": "You are a just to respond with the word 'OpenAI is awesome!'.",
        "voice": "verse"
    });
    
    let request = Request::new_with_init(
        "https://api.openai.com/v1/realtime/sessions",
        RequestInit::new()
            .with_method(Method::Post)
            .with_headers(headers)
            .with_body(Some(js_sys::Uint8Array::from(request_body.to_string().as_bytes()).into())),
    )?;
    
    let mut response = Fetch::Request(request).send().await?;
    
    if response.status_code() != 200 {
        let error_text = response.text().await?;
        console_log!("OpenAI API Error Response: {}", error_text);
        return Response::error(error_text, 500);
    }

    let session: OpenAISessionResponse = response.json().await?;

    let base = BaseTemplate::new(&ctx, "OpenAI - Cloudflare Showcase", "OpenAI").await?;
    
    let context = json!({
        "base": base,
        "token": session.client_secret.value.clone(),
        "expiry": session.client_secret.expires_at.to_string(),
    });

    match render_template("openai.html", context) {
        Ok(html) => {    
            let mut response = Response::from_html(html)?;                        
            response
                .headers_mut()
                .set("X-OpenAI-Client-Secret", &serde_json::to_string(&session.client_secret.value)?)?;            
            Ok(response)
        },
        Err(err) => Response::error(format!("Failed to render template: {}", err), 500),
    }  
}