use askama::Template;
use worker::*;
use serde_json::{json, Value};

#[derive(Template)]
#[template(path = "openai.html")]
struct OpenAITemplate {
    title: String,
    page_title: String,
    current_year: String,
    version: String,
    token: String,
}

pub async fn handler(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    match req.method() {
        Method::Get => {
            let api_key = ctx.env.secret("OPENAI_API_KEY")?.to_string();
            
            let client = reqwest::Client::new();
            let response = client
                .post("https://api.openai.com/v1/realtime/sessions")
                .header("Authorization", format!("Bearer {}", api_key))
                .header("Content-Type", "application/json")
                .json(&json!({
                    "model": "gpt-4o-realtime-preview-2024-12-17",
                    "voice": "verse"
                }))
                .send()
                .await
                .map_err(|e| Error::from(e.to_string()))?;

            let token = if response.status().is_success() {
                let json: Value = response.json().await.map_err(|e| Error::from(e.to_string()))?;
                json["client_secret"]["value"].as_str()
                    .ok_or_else(|| Error::from("Missing client_secret value"))?
                    .to_string()
            } else {
                let error_text = response.text().await.map_err(|e| Error::from(e.to_string()))?;
                return Response::error(error_text, 500);
            };

            let template = OpenAITemplate {
                title: "OpenAI - Cloudflare Showcase".to_string(),
                page_title: "OpenAI".to_string(),
                current_year: "2024".to_string(),
                version: option_env!("CARGO_PKG_VERSION").unwrap_or_default().to_string(),
                token,
            };

            match template.render() {
                Ok(html) => Response::from_html(html),
                Err(err) => Response::error(format!("Failed to render template: {}", err), 500),
            }
        }
        _ => Response::error("Method not allowed", 405),
    }
} 