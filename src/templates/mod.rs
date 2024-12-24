use askama::Template;

#[derive(Template)]
#[template(path = "components/clock.rs.html")]
pub struct ClockTemplate {
    pub utc: u64,
} 