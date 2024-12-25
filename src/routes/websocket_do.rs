use worker::*;
use worker::console_log;
use serde_json::Value;
use wasm_bindgen::prelude::*;
use std::sync::{Arc, Mutex};
use async_std::stream::StreamExt;
use std::time::Duration;
use async_std::task;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
enum ControlValue {
    Bool(bool),
    Text(String),
}

#[derive(Serialize, Deserialize, Debug)]
struct ControlMessage {
    control_name: String,
    value: ControlValue,
    control_type: String,
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

impl SharedState {
    fn broadcast_client_count(&self) {
        let json_string = serde_json::to_string(&serde_json::json!({
            "id": "client-count",
            "value": self.clients.len()
        })).expect("Failed to serialize client_count");

        for client in &self.clients {
            if let Err(e) = client.socket.send_with_str(&json_string) {
                console_log!("Failed to send message to client {}: {}", client.id, e);
            }
        }
    }

    fn broadcast_time(&self) {
        let json_string = serde_json::to_string(&serde_json::json!({
            "id": "clock-display",
            "utc": Date::now().as_millis()
        })).expect("Failed to serialize time");

        for client in &self.clients {
            if let Err(e) = client.socket.send_with_str(&json_string) {
                console_log!("Failed to send time update to client {}: {}", client.id, e);
            }
        }
    }
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
        let url = req.url()?;

        if !req.headers().get("Upgrade")?.map_or(false, |v| v.eq_ignore_ascii_case("websocket")) {
            return Response::error("Expected Upgrade: websocket", 426);
        }

        let pair = WebSocketPair::new().map_err(|e| worker::Error::RustError(e.to_string()))?;
        let server = pair.server;
        let client = pair.client;

        server.accept()?;

        let client_id = url.query_pairs()
            .find(|(key, _)| key == "id")
            .map(|(_, value)| value.to_string())
            .unwrap_or_else(|| format!("client-{}", Date::now().as_millis()));

        let shared = Arc::clone(&self.shared);
        let client_id_clone = client_id.clone();
        let server_clone = server.clone();

        wasm_bindgen_futures::spawn_local(async move {
            let mut event_stream = match server_clone.events() {
                Ok(stream) => stream,
                Err(e) => {
                    console_log!("Failed to create event stream: {}", e);
                    return;
                }
            };

            while let Some(event) = event_stream.next().await {
                match event {
                    Ok(WebsocketEvent::Message(msg)) => {
                        let text = match msg.text() {
                            Some(text) => text,
                            None => continue,
                        };

                        let shared = shared.lock().unwrap();
                        shared.clients
                            .iter()
                            .filter(|c| c.id != client_id_clone)
                            .for_each(|client| {
                                if let Err(e) = client.socket.send_with_str(&text) {
                                    console_log!("Failed to send message to client {}: {}", client.id, e);
                                }
                            });
                    }
                    Ok(WebsocketEvent::Close(_)) | Err(_) => {
                        let mut shared = shared.lock().unwrap();
                        shared.clients.retain(|c| c.id != client_id_clone);
                        shared.broadcast_client_count();
                        break;
                    }
                }
            }
        });

        let mut shared = self.shared.lock().unwrap();
        shared.clients.push(Client { id: client_id.clone(), socket: server.clone() });
        shared.broadcast_client_count();

        if !shared.clock_started {
            shared.clock_started = true;
            let shared = Arc::clone(&self.shared);

            wasm_bindgen_futures::spawn_local(async move {
                loop {
                    task::sleep(Duration::from_secs(1)).await;

                    let mut shared = shared.lock().unwrap();
                    if shared.clients.is_empty() {
                        shared.clock_started = false;
                        break;
                    }

                    shared.broadcast_time();
                }
            });
        }

        Response::from_websocket(client)
    }
}

pub async fn handler(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let namespace = ctx.env.durable_object("WebsocketDO")?;
    let stub = namespace.id_from_name("WebsocketDO")?.get_stub()?;
    stub.fetch_with_request(req).await
}
