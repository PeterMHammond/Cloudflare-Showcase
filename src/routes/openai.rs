use askama::Template;
use worker::*;
use serde_json::{json, Value};
use crate::template::{BaseTemplate, DefaultBaseTemplate};

#[derive(Template)]
#[template(path = "openai.html")]
pub struct OpenAITemplate {
    inner: DefaultBaseTemplate,
    token: String,
}

impl BaseTemplate for OpenAITemplate {
    fn title(&self) -> &str { self.inner.title() }
    fn page_title(&self) -> &str { self.inner.page_title() }
    fn current_year(&self) -> &str { self.inner.current_year() }
    fn version(&self) -> &str { self.inner.version() }
}

pub async fn handler(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    match req.method() {
        Method::Get => {
            let base = DefaultBaseTemplate::default();
            
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
                inner: base,
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