use cookie::{Cookie, CookieJar, Key};
use worker::*;
use routes::{
    about::handler as about,
    index::handler as index,
    favicon::handler as favicon,
    websocket_do::handler as websocket_do,
    websocket::handler as websocket,
    study::handler as study,
    study_do::handler as study_do,
    openai::handler as openai,
    turnstile,
};

pub struct BaseTemplate {
    pub title: String,
    pub page_title: String,
    pub site_key: String,
    pub current_year: String,
    pub version: String,
}

impl BaseTemplate {
    pub async fn new(ctx: &RouteContext<()>, title: &str, page_title: &str) -> Result<Self> {
        let site_key = ctx.env.secret("TURNSTILE_SITE_KEY")?.to_string();
        
        Ok(Self {
            title: title.to_string(),
            page_title: page_title.to_string(),
            current_year: "2024".to_string(),
            version: option_env!("CARGO_PKG_VERSION").unwrap_or_default().to_string(),
            site_key,
        })
    }
}

pub mod utils {
    pub mod scripture;
    pub mod turnstile;
}
pub mod routes {
    pub mod about;
    pub mod index;
    pub mod favicon;
    pub mod websocket;
    pub mod websocket_do;
    pub mod study;
    pub mod study_do;
    pub mod openai;
    pub mod turnstile;
}

#[event(fetch)]
async fn fetch(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    console_error_panic_hook::set_once();

    let value = match req.headers().get("Cookie")? {
        Some(header) => {
            let header_str = header.to_string();
            let secret_key = env.secret("TURNSTILE_SECRET_KEY")?.to_string();
            let secret_bytes = secret_key.as_bytes();
            let cookie_key = Key::derive_from(secret_bytes);    
            let mut jar = CookieJar::new();
            for cookie_str in header_str.split(';').map(|s| s.trim().to_owned()) {
                if let Ok(cookie) = Cookie::parse(cookie_str) {
                    jar.add_original(cookie);
                }
            }
            match jar.signed(&cookie_key).get("turnstile") {
                Some(cookie) => cookie.value().to_string(),
                None => "Invalid Turnstile".to_string(),
            }
        }
        None => "Cookie header not found".to_string(),
    };
    
    console_log!("Cookie value: {}", value);

    let router = Router::with_data(());
    
    let response = router
        .get_async("/", index)
        .get_async("/favicon.ico", favicon)
        .get_async("/about", about)
        .get_async("/websocket_do", websocket_do)
        .get_async("/websocket", websocket)
        .get_async("/study", study)
        .get_async("/study_do", study_do)
        .get_async("/openai", openai)
        .get_async("/turnstile", turnstile::get_handler)
        .post_async("/turnstile", turnstile::post_handler)
        .run(req, env)
        .await?;

    if response.status_code() == 404 {
        return Response::from_html(
            "<h1>404 - God exists, but this page doesn't.</h1><p>For since the creation of the world His invisible attributes, both His eternal power and divine nature, have been clearly seen, being understood through what has been made, so that they are without excuse. - Romans 1:20 LSB</p>"
        ).map(|resp| resp.with_status(404));
    }

    Ok(response)
}