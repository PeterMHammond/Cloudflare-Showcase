use askama::Template;
use worker::*;
use crate::BaseTemplate;

#[derive(Template)]
#[template(path = "about.html")]
struct AboutTemplate {
    #[template(name = "base")]
    base: BaseTemplate,
}

pub async fn handler(_req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let base = BaseTemplate::new(&ctx, "About - Cloudflare Showcase", "About").await?;
    
    let template = AboutTemplate { base };

    match template.render() {
        Ok(html) => Response::from_html(html),
        Err(err) => Response::error(format!("Failed to render template: {}", err), 500),
    }
} 