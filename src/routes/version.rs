use worker::*;

// For development usage to show the version of the application TODO: remove this before production deployment
pub async fn handler(req: Request, _ctx: RouteContext<()>) -> Result<Response> {
    let url = req.url()?;
    let root_url = format!("{}://{}/", url.scheme(), url.host_str().unwrap_or("localhost"));

    let version = option_env!("CARGO_PKG_VERSION").unwrap_or_default();
    let name = option_env!("CARGO_PKG_NAME").unwrap_or_default();
    let authors = option_env!("CARGO_PKG_AUTHORS").unwrap_or_default();
    let description = option_env!("CARGO_PKG_DESCRIPTION").unwrap_or_default();
    let repository = option_env!("CARGO_PKG_REPOSITORY").unwrap_or_default();
    let license = option_env!("CARGO_PKG_LICENSE").unwrap_or_default();

    let response_body = format!(
        "Package: {name}\n\
        Version: {version}\n\
        Authors: {authors}\n\
        Description: {description}\n\
        Repository: {repository}\n\
        License: {license}\n\
        Current Route: {url}\n\n\
        Available Routes:\n\
        - GET /version\n    Shows this diagnostic information\n    Example: curl -X GET {root_url}version\n\
        - GET /websocket\n    Interactive WebSocket demo page\n    Example: Visit {root_url}websocket\n\
        - WS /websocket_do\n    WebSocket connection endpoint (Durable Object):\n\
          * Real-time clock updates\n\
          * Client count tracking\n\
          * Shared form controls (checkbox, textarea)\n\
          * Debug logging\n\
          Note: This is a WebSocket endpoint, not accessible via HTTP\n"
    );

    Response::ok(response_body)
} 