use askama::Template;

pub trait BaseTemplate {
    fn title(&self) -> &str;
    fn page_title(&self) -> &str;
    fn current_year(&self) -> &str;
    fn version(&self) -> &str;
}

#[derive(Template)]
#[template(path = "base.html")]
pub struct DefaultBaseTemplate {
    pub title: String,
    pub page_title: String,
    pub current_year: String,
    pub version: String,
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