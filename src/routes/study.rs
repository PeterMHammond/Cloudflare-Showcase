use askama::Template;
use worker::*;
use crate::template::{BaseTemplate, DefaultBaseTemplate};

#[derive(Template)]
#[template(path = "study.html")]
struct StudyTemplate {
    base: DefaultBaseTemplate,
}

impl BaseTemplate for StudyTemplate {
    fn title(&self) -> &str { self.base.title() }
    fn page_title(&self) -> &str { self.base.page_title() }
    fn current_year(&self) -> &str { self.base.current_year() }
    fn version(&self) -> &str { self.base.version() }
}

pub async fn handler(_req: Request, _ctx: RouteContext<()>) -> Result<Response> {
    let mut base = DefaultBaseTemplate::default();
    base.title = "Study - Cloudflare Showcase".to_string();
    base.page_title = "Study".to_string();

    let template = StudyTemplate { base };

    match template.render() {
        Ok(html) => Response::from_html(html),
        Err(err) => Response::error(format!("Failed to render template: {}", err), 500),
    }
} 