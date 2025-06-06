use worker::*;
use utils::middleware::ValidationState;
use routes::{
    about::handler as about,
    analytics::handler as analytics,
    analytics_api::metrics_handler as analytics_api,
    analytics_dashboard::handler as analytics_dashboard,
    index::handler as index,
    websocket_do::handler as websocket_do,
    websocket::handler as websocket,
    study::handler as study,
    study_do::handler as study_do,
    openai::handler as openai,
    stt::handler as stt,
    stt::do_handler::handler as stt_do,
    turnstile,
    verify,
    version::handler as version,
    sqlite::handler as sqlite,
    sqlite::api_handler as sqlite_api,
    sqlite_test::handle as sqlite_test,
};
use serde::Serialize;

#[derive(Serialize)]
pub struct BaseTemplate {
    pub title: String,
    pub page_title: String,
    pub site_key: String,
    pub current_year: String,
    pub version: String,
    pub is_validated: bool,
    pub validation_message: String,
}

impl BaseTemplate {
    pub async fn new(ctx: &RouteContext<ValidationState>, title: &str, page_title: &str) -> Result<Self> {
        let site_key = ctx.env.secret("TURNSTILE_SITE_KEY")?.to_string();
        
        Ok(Self {
            title: title.to_string(),
            page_title: page_title.to_string(),
            current_year: "2024".to_string(),
            version: option_env!("CARGO_PKG_VERSION").unwrap_or_default().to_string(),
            site_key,
            is_validated: ctx.data.is_validated,
            validation_message: ctx.data.validation_message.to_string(),
        })
    }
}

pub mod utils {
    pub mod scripture;
    pub mod turnstile;
    pub mod middleware;
    pub mod templates;
    pub mod sql_bindings;
}
pub mod routes;

#[event(fetch)]
async fn fetch(req: Request, env: Env, ctx: Context) -> Result<Response> {
    console_error_panic_hook::set_once();

    let (req, validation_state) = utils::middleware::validate_turnstile(req, &env, &ctx).await?;
    console_log!("Validation state: {}", validation_state.validation_message);

    let router = Router::with_data(validation_state);
    
    let response = router
        .get_async("/", index)
        .get_async("/about", about)
        .get_async("/analytics", analytics)
        .get_async("/analytics/dashboard", analytics_dashboard)
        .get_async("/analytics/api/metrics", analytics_api)
        .post_async("/analytics/data", routes::analytics::data_handler)
        .get_async("/websocket_do", websocket_do)
        .get_async("/websocket", websocket)
        .get_async("/study", study)
        .get_async("/study_do", study_do)
        .get_async("/openai", openai)
        .get_async("/stt", stt)
        .get_async("/stt/ws", stt_do)
        .get_async("/turnstile", turnstile::get_handler)
        .post_async("/turnstile", turnstile::post_handler)
        .get_async("/verify", verify::get_handler)
        .post_async("/verify", verify::post_handler)
        .get_async("/version", version)
        .get_async("/sqlite", sqlite)
        .get_async("/sqlite/test", sqlite_test)
        .get_async("/sqlite/api/messages", sqlite_api)
        .get_async("/sqlite/api/user/:id", sqlite_api)
        .get_async("/sqlite/api/stats", sqlite_api)
        .get_async("/sqlite/api/export", sqlite_api)
        .get_async("/sqlite/api/sql-test", sqlite_api)
        .post_async("/sqlite/api/message", sqlite_api)
        .delete_async("/sqlite/api/old", sqlite_api)
        .delete_async("/sqlite/api/messages", sqlite_api)
        .run(req, env)
        .await?;

    if response.status_code() == 404 {
        return Response::from_html(
            "<h1>404 - God exists, but this page doesn't.</h1><p>For since the creation of the world His invisible attributes, both His eternal power and divine nature, have been clearly seen, being understood through what has been made, so that they are without excuse. - Romans 1:20 LSB</p>"
        ).map(|resp| resp.with_status(404));
    }

    Ok(response)
}