use worker::*;
use serde_json::json;
use wasm_bindgen::prelude::*;
use std::time::Duration;
use async_std::task;
use std::sync::{Arc, Mutex};
use askama::Template;
use crate::template::{BaseTemplate, DefaultBaseTemplate};

#[derive(Template)]
#[template(path = "websocket.html")]
struct WebsocketTemplate {
    inner: DefaultBaseTemplate,
}

impl BaseTemplate for WebsocketTemplate {
    fn title(&self) -> &str { self.inner.title() }
    fn page_title(&self) -> &str { self.inner.page_title() }
    fn current_year(&self) -> &str { self.inner.current_year() }
    fn version(&self) -> &str { self.inner.version() }
}

#[derive(Clone)]
struct Client {
    id: String,
    socket: WebSocket,
}

struct SharedState {
    clients: Vec<Client>,
    clock_started: bool,
}

#[wasm_bindgen]
pub struct WebsocketDO {
    state: State,
    shared: Arc<Mutex<SharedState>>,
}

#[durable_object]
impl DurableObject for WebsocketDO {
    fn new(state: State, _env: Env) -> Self {
        Self { 
            state,
            shared: Arc::new(Mutex::new(SharedState {
                clients: Vec::new(),
                clock_started: false,
            })),
        }
    }

    async fn fetch(&mut self, mut req: Request) -> Result<Response> {
        let url = req.url()?;
        
        // Check if this is a WebSocket upgrade request
        if !req.headers().get("Upgrade")?.map_or(false, |v| v.eq_ignore_ascii_case("websocket")) {
            return Response::error("Expected Upgrade: websocket", 426);
        }

        match WebSocketPair::new() {
            Ok(pair) => {
                let server = pair.server;
                let client = pair.client;

                // Accept the WebSocket connection
                server.accept()?;

                // Get or generate client ID
                let client_id = url.query_pairs()
                    .find(|(key, _)| key == "id")
                    .map(|(_, value)| value.to_string())
                    .unwrap_or_else(|| format!("client-{}", Date::now().as_millis()));

                // Send the client ID back to the client
                let id_message = json!({
                    "type": "client_id",
                    "id": client_id
                }).to_string();
                server.send_with_str(&id_message)?;

                // Add the client and check if we need to start the clock
                let start_clock = {
                    let mut shared = self.shared.lock().unwrap();
                    
                    // Remove any existing connection with the same ID
                    shared.clients.retain(|c| {
                        if c.id == client_id {
                            return false;
                        }
                        // Test connection with ping message
                        match c.socket.send_with_str(&json!({"type": "ping"}).to_string()) {
                            Ok(_) => true,
                            Err(_) => {
                                console_log!("Removing disconnected client {}: ping failed", c.id);
                                false
                            }
                        }
                    });
                    
                    // Add the new client
                    shared.clients.push(Client {
                        id: client_id,
                        socket: server.clone(),
                    });

                    !shared.clock_started
                };

                // Start the global clock if needed
                if start_clock {
                    let shared = Arc::clone(&self.shared);
                    
                    // Start the message sending loop
                    wasm_bindgen_futures::spawn_local(async move {
                        loop {
                            let timestamp = Date::now().as_millis();
                            let message = json!({
                                "type": "timestamp",
                                "timestamp": timestamp
                            }).to_string();
                            
                            let mut shared = shared.lock().unwrap();
                            shared.clients.retain(|client| {
                                // First try a ping message
                                if let Err(_) = client.socket.send_with_str(&json!({"type": "ping"}).to_string()) {
                                    console_log!("Removing disconnected client {}: ping failed", client.id);
                                    return false;
                                }
                                
                                // Then try to send the message
                                match client.socket.send_with_str(&message) {
                                    Ok(_) => true,
                                    Err(e) => {
                                        console_log!("Removing disconnected client {}: send failed - {:?}", client.id, e);
                                        false
                                    }
                                }
                            });
                            
                            if shared.clients.is_empty() {
                                shared.clock_started = false;
                                break;
                            }
                            
                            drop(shared); // Release the lock before sleeping
                            task::sleep(Duration::from_secs(1)).await;
                        }
                    });
                    
                    // Mark the clock as started
                    self.shared.lock().unwrap().clock_started = true;
                }

                Response::from_websocket(client)
            }
            Err(e) => Response::error(e.to_string(), 500)
        }
    }
}

pub mod websocket {
    use super::*;

    pub async fn handler(_req: Request, _ctx: RouteContext<()>) -> Result<Response> {
        let base = DefaultBaseTemplate::default();
        let template = WebsocketTemplate { inner: base };

        match template.render() {
            Ok(html) => Response::from_html(html),
            Err(err) => Response::error(format!("Failed to render template: {}", err), 500),
        }
    }
}

pub async fn handler(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let namespace = ctx.env.durable_object("WebsocketDO")?;
    let stub = namespace.id_from_name("WebsocketDO")?.get_stub()?;
    stub.fetch_with_request(req).await
} 