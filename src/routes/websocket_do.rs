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
    last_pong: u64,
    last_ping: u64,
}

struct SharedState {
    clients: Vec<Client>,
    clock_started: bool,
}

#[wasm_bindgen]
pub struct WebsocketDO {
    shared: Arc<Mutex<SharedState>>,
}

#[durable_object]
impl DurableObject for WebsocketDO {
    fn new(state: State, _env: Env) -> Self {
        Self { 
            shared: Arc::new(Mutex::new(SharedState {
                clients: Vec::new(),
                clock_started: false,
            })),
        }
    }

    async fn fetch(&mut self, req: Request) -> Result<Response> {
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
                let shared = Arc::clone(&self.shared);

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
                                                let mut shared = shared.lock().unwrap();
                                                if let Some(client) = shared.clients.iter_mut().find(|c| c.id == client_id_clone) {
                                                    client.last_pong = Date::now().as_millis();
                                                }
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
                                let mut shared = shared.lock().unwrap();
                                let previous_count = shared.clients.len();
                                shared.clients.retain(|c| c.id != client_id_clone);
                                
                                // Broadcast updated client count after close
                                if shared.clients.len() != previous_count {
                                    let client_count_msg = json!({
                                        "type": "client_count",
                                        "count": shared.clients.len()
                                    }).to_string();
                                    
                                    for client in &shared.clients {
                                        let _ = client.socket.send_with_str(&client_count_msg);
                                    }
                                }
                                break;
                            },
                            Err(e) => {
                                console_log!("Error receiving message from client {}: {:?}", client_id_clone, e);
                                let mut shared = shared.lock().unwrap();
                                let previous_count = shared.clients.len();
                                shared.clients.retain(|c| c.id != client_id_clone);
                                
                                // Broadcast updated client count after error
                                if shared.clients.len() != previous_count {
                                    let client_count_msg = json!({
                                        "type": "client_count",
                                        "count": shared.clients.len()
                                    }).to_string();
                                    
                                    for client in &shared.clients {
                                        let _ = client.socket.send_with_str(&client_count_msg);
                                    }
                                }
                                break;
                            }
                        }
                    }
                });

                let now = Date::now().as_millis();
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
                    
                    // Stagger initial ping time based on client ID hash
                    let stagger_offset = client_id.bytes().fold(0u64, |acc, b| acc.wrapping_add(b as u64)) % 30000;
                    let initial_ping = now.saturating_sub(stagger_offset);
                    
                    let previous_count = shared.clients.len();
                    shared.clients.push(Client {
                        id: client_id,
                        socket: server.clone(),
                        last_pong: now,
                        last_ping: initial_ping,
                    });

                    // Only broadcast if count changed
                    if shared.clients.len() != previous_count {
                        let client_count_msg = json!({
                            "type": "client_count",
                            "count": shared.clients.len()
                        }).to_string();
                        
                        for client in &shared.clients {
                            let _ = client.socket.send_with_str(&client_count_msg);
                        }
                    }

                    !shared.clock_started
                };

                if start_clock {
                    let shared = Arc::clone(&self.shared);
                    
                    wasm_bindgen_futures::spawn_local(async move {
                        loop {
                            let now = Date::now().as_millis();
                            let mut shared = shared.lock().unwrap();
                            
                            // Process all clients in a single pass
                            let mut i = 0;
                            while i < shared.clients.len() {
                                let client = &mut shared.clients[i];
                                let client_id = client.id.clone();
                                
                                // Check if it's time to ping this client
                                if now - client.last_ping >= 30000 {
                                    // Check for ping timeout
                                    if now - client.last_pong >= 45000 {
                                        console_log!("Removing client {} due to ping timeout", client_id);
                                        let previous_count = shared.clients.len();
                                        shared.clients.remove(i);
                                        
                                        // Only broadcast if count changed
                                        if shared.clients.len() != previous_count {
                                            let client_count_msg = json!({
                                                "type": "client_count",
                                                "count": shared.clients.len()
                                            }).to_string();
                                            
                                            for client in &shared.clients {
                                                let _ = client.socket.send_with_str(&client_count_msg);
                                            }
                                        }
                                        
                                        continue;
                                    }
                                    
                                    // Send ping
                                    match client.socket.send_with_str(&json!({"type": "ping"}).to_string()) {
                                        Ok(_) => {
                                            client.last_ping = now;
                                            i += 1;
                                        },
                                        Err(e) => {
                                            console_log!("Removing client {}: ping failed - {:?}", client_id, e);
                                            let previous_count = shared.clients.len();
                                            shared.clients.remove(i);
                                            
                                            // Only broadcast if count changed
                                            if shared.clients.len() != previous_count {
                                                let client_count_msg = json!({
                                                    "type": "client_count",
                                                    "count": shared.clients.len()
                                                }).to_string();
                                                
                                                for client in &shared.clients {
                                                    let _ = client.socket.send_with_str(&client_count_msg);
                                                }
                                            }
                                            
                                            continue;
                                        }
                                    }
                                } else {
                                    i += 1;
                                }
                            }
                            
                            // Send timestamp to all remaining clients
                            let timestamp_msg = json!({
                                "type": "timestamp",
                                "timestamp": now
                            }).to_string();
                            
                            let mut i = 0;
                            while i < shared.clients.len() {
                                let client = &shared.clients[i];
                                match client.socket.send_with_str(&timestamp_msg) {
                                    Ok(_) => i += 1,
                                    Err(e) => {
                                        console_log!("Removing client {}: timestamp failed - {:?}", client.id, e);
                                        let previous_count = shared.clients.len();
                                        shared.clients.remove(i);
                                        
                                        // Broadcast updated client count after timestamp failure
                                        if shared.clients.len() != previous_count {
                                            let client_count_msg = json!({
                                                "type": "client_count",
                                                "count": shared.clients.len()
                                            }).to_string();
                                            
                                            for client in &shared.clients {
                                                let _ = client.socket.send_with_str(&client_count_msg);
                                            }
                                        }
                                    }
                                }
                            }
                            
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