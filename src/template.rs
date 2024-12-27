pub trait BaseTemplate {
    fn title(&self) -> &str;
    fn page_title(&self) -> &str;
    fn current_year(&self) -> &str;
    fn version(&self) -> &str;
}

pub struct DefaultBaseTemplate {
    title: String,
    page_title: String,
    current_year: String,
    version: String,
}

impl Default for DefaultBaseTemplate {
    fn default() -> Self {
        Self {
            title: String::from("Cloudflare Showcase"),
            page_title: String::from("Welcome"),
            current_year: String::from("2024"),
            version: option_env!("CARGO_PKG_VERSION").unwrap_or_default().to_string(),
        }
    }
}

impl BaseTemplate for DefaultBaseTemplate {
    fn title(&self) -> &str { &self.title }
    fn page_title(&self) -> &str { &self.page_title }
    fn current_year(&self) -> &str { &self.current_year }
    fn version(&self) -> &str { &self.version }
}

use askama::Template;

#[derive(Template)]
#[template(path = "study.html")]
pub struct StudyTemplate {}

impl BaseTemplate for StudyTemplate {
    fn title(&self) -> &str { "Study - Cloudflare Showcase" }
    fn page_title(&self) -> &str { "Study" }
    fn current_year(&self) -> &str { "2024" }
    fn version(&self) -> &str { option_env!("CARGO_PKG_VERSION").unwrap_or_default() }
} 