use worker::*;
use serde_json::json;
use wasm_bindgen::prelude::*;
use std::time::Duration;
use async_std::task;
use std::sync::{Arc, Mutex};
use async_std::stream::StreamExt;

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
        console_log!("🟢 Fetch method invoked.");
        let url = req.url()?;
        
        if !req.headers().get("Upgrade")?.map_or(false, |v| v.eq_ignore_ascii_case("websocket")) {
            return Response::error("Expected Upgrade: websocket", 426);
        }

        match WebSocketPair::new() {
            Ok(pair) => {
                let server = pair.server;
                let client = pair.client;

                server.accept()?;

                let client_id = url.query_pairs()
                    .find(|(key, _)| key == "id")
                    .map(|(_, value)| value.to_string())
                    .unwrap_or_else(|| format!("client-{}", Date::now().as_millis()));

                let id_message = json!({
                    "type": "client_id",
                    "id": client_id
                }).to_string();
                server.send_with_str(&id_message)?;

                let server_clone = server.clone();
                let client_id_clone = client_id.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    let mut event_stream = server_clone.events().expect("Failed to get event stream");
                    
                    while let Some(event) = event_stream.next().await {
                        match event {
                            Ok(WebsocketEvent::Message(msg)) => {
                                if let Some(text) = msg.text() {
                                    if let Ok(data) = serde_json::from_str::<serde_json::Value>(&text) {
                                        match data.get("type").and_then(|t| t.as_str()) {
                                            Some("pong") => {
                                                console_log!("Received pong from client {}", client_id_clone);
                                            },
                                            Some("client_id_ack") => {
                                                console_log!("Client {} acknowledged ID", client_id_clone);
                                            },
                                            Some(msg_type) => {
                                                console_log!("Received message type {} from client {}", msg_type, client_id_clone);
                                            },
                                            None => {
                                                console_log!("Received message without type from client {}", client_id_clone);
                                            }
                                        }
                                    }
                                }
                            },
                            Ok(WebsocketEvent::Close(_)) => {
                                console_log!("Client {} connection closed", client_id_clone);
                                break;
                            },
                            Err(e) => {
                                console_log!("Error receiving message from client {}: {:?}", client_id_clone, e);
                                break;
                            }
                        }
                    }
                });

                let start_clock = {
                    let mut shared = self.shared.lock().unwrap();
                    
                    shared.clients.retain(|c| {
                        if c.id == client_id {
                            return false;
                        }
                        match c.socket.send_with_str(&json!({"type": "ping"}).to_string()) {
                            Ok(_) => true,
                            Err(_) => {
                                console_log!("Removing disconnected client {}: ping failed", c.id);
                                false
                            }
                        }
                    });
                    
                    shared.clients.push(Client {
                        id: client_id,
                        socket: server.clone(),
                    });

                    !shared.clock_started
                };

                if start_clock {
                    let shared = Arc::clone(&self.shared);
                    
                    wasm_bindgen_futures::spawn_local(async move {
                        loop {
                            let timestamp = Date::now().as_millis();
                            let message = json!({
                                "type": "timestamp",
                                "timestamp": timestamp
                            }).to_string();
                            
                            let mut shared = shared.lock().unwrap();
                            shared.clients.retain(|client| {
                                if let Err(_) = client.socket.send_with_str(&json!({"type": "ping"}).to_string()) {
                                    console_log!("Removing disconnected client {}: ping failed", client.id);
                                    return false;
                                }
                                
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
                            
                            drop(shared);
                            task::sleep(Duration::from_secs(1)).await;
                        }
                    });
                    
                    self.shared.lock().unwrap().clock_started = true;
                }

                Response::from_websocket(client)
            }
            Err(e) => Response::error(e.to_string(), 500)
        }
    }
}

pub async fn handler(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let namespace = ctx.env.durable_object("WebsocketDO")?;
    let stub = namespace.id_from_name("WebsocketDO")?.get_stub()?;
    stub.fetch_with_request(req).await
} 