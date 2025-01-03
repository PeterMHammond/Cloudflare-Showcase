use askama::Template;
use worker::*;

#[derive(Template)]
#[template(path = "version.html")]
struct VersionTemplate {
    title: String,
    page_title: String,
    current_year: String,
    version: String,
}

pub async fn handler(_req: Request, _ctx: RouteContext<()>) -> Result<Response> {
    let template = VersionTemplate {
        title: "Version - Cloudflare Showcase".to_string(),
        page_title: "Version".to_string(),
        current_year: "2024".to_string(),
        version: option_env!("CARGO_PKG_VERSION").unwrap_or_default().to_string(),
    };

    match template.render() {
        Ok(html) => Response::from_html(html),
        Err(err) => Response::error(format!("Failed to render template: {}", err), 500),
    }
} 