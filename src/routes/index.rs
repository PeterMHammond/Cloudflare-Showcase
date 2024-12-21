use askama::Template;
use worker::*;

#[derive(Template)]
#[template(path = "index.html")]
struct IndexTemplate {
    title: String,
}

pub async fn handler(_req: Request, _ctx: RouteContext<()>) -> Result<Response> {
    let template = IndexTemplate {
        title: String::from("Cloudflare Showcase"),
    };

    match template.render() {
        Ok(html) => Response::from_html(html),
        Err(err) => Response::error(format!("Failed to render template: {}", err), 500),
    }
} 