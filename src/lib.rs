use worker::*;

mod template;
pub mod routes {
    pub mod about;
    pub mod index;
}

#[event(fetch)]
async fn fetch(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    console_error_panic_hook::set_once();
    
    let router = Router::new();

    let response = router
        .get_async("/", routes::index::handler)
        .get_async("/about", routes::about::handler)
        .run(req, env)
        .await?;

    if response.status_code() == 404 {
        return Response::from_html(
            "<h1>404 - God exists, but this page doesn't.</h1><p>For since the creation of the world His invisible attributes, both His eternal power and divine nature, have been clearly seen, being understood through what has been made, so that they are without excuse. - Romans 1:20 LSB</p>"
        ).map(|resp| resp.with_status(404));
    }

    Ok(response)
}