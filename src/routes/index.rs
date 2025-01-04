use askama::Template;
use worker::*;
use crate::BaseTemplate;
use crate::utils::middleware::ValidationState;

#[derive(Template)]
#[template(path = "index.html")]
struct IndexTemplate {
    #[template(name = "base")]
    base: BaseTemplate,
}

pub async fn handler(_req: Request, ctx: RouteContext<ValidationState>) -> Result<Response> {
    let base = BaseTemplate::new(&ctx, "Home - Cloudflare Showcase", "Welcome").await?;
    
    let template = IndexTemplate { base };

    match template.render() {
        Ok(html) => Response::from_html(html),
        Err(err) => Response::error(format!("Failed to render template: {}", err), 500),
    }
} 