use askama::Template;
use worker::*;
use crate::template::{BaseTemplate, DefaultBaseTemplate};

#[derive(Template)]
#[template(path = "index.html")]
pub struct IndexTemplate {
    inner: DefaultBaseTemplate,
}

impl BaseTemplate for IndexTemplate {
    fn title(&self) -> &str { self.inner.title() }
    fn page_title(&self) -> &str { self.inner.page_title() }
    fn current_year(&self) -> &str { self.inner.current_year() }
    fn version(&self) -> &str { self.inner.version() }
}

pub async fn handler(_req: Request, _ctx: RouteContext<()>) -> Result<Response> {
    let base = DefaultBaseTemplate::default();
    let template = IndexTemplate { inner: base };

    match template.render() {
        Ok(html) => Response::from_html(html),
        Err(err) => Response::error(format!("Failed to render template: {}", err), 500),
    }
} 