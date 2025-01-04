use worker::*;
use crate::utils::middleware::ValidationState;

pub async fn handler(_req: Request, _ctx: RouteContext<ValidationState>) -> Result<Response> {
    let response_body = include_bytes!("../favicon.ico").to_vec();
    let mut headers = Headers::new();
    headers.append("Content-Type", "image/x-icon")?;
    headers.append("Cache-Control", "public, max-age=31536000")?;
    Ok(Response::from_bytes(response_body)?
        .with_headers(headers))
}